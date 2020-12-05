#![cfg_attr(not(feature = "std"), no_std)]

use alt_serde::{Deserialize, Deserializer};
use codec::{Decode, Encode};
use core::convert::TryInto;
use frame_support::traits::{BalanceStatus, ReservableCurrency};
use frame_support::{
    debug, decl_error, decl_event, decl_module, decl_storage, dispatch, ensure, traits::Randomness,
    StorageMap,
};
use frame_system::{
    ensure_signed,
    offchain::{AppCrypto, CreateSignedTransaction, ForAll, SendSignedTransaction, Signer},
};
use sp_core::crypto::{AccountId32, KeyTypeId};
use sp_io::hashing::blake2_128;
use sp_runtime::offchain::storage::StorageValueRef;
use sp_runtime::traits::IdentifyAccount;
use sp_runtime::RuntimeAppPublic;
use sp_std::prelude::*;
use sp_std::str;
use uuid::{Builder, Uuid, Variant, Version};

#[cfg(test)]
mod mock;

#[cfg(test)]
mod tests;

#[cfg(feature = "std")]
mod delegate;
#[cfg(feature = "std")]
mod error;
#[cfg(feature = "std")]
mod http;
#[cfg(feature = "std")]
mod storage;
#[cfg(feature = "std")]
mod task;

pub const SERVICE_BASE_URL: &'static str = "http://localhost:8000";
pub const SERVICE_BASE_URL_PREFIX: &'static str = "http://";

pub const KEY_TYPE: KeyTypeId = KeyTypeId(*b"demo");
pub const TEA_SEND_TASK_TIMEOUT_PERIOD: u64 = 3000;

pub const LOCAL_STORAGE_TASKS_RESULTS_KEY: &'static str = "local-storage::tasks_results";
pub const LOCAL_STORAGE_TASKS_RESULTS_LOCK: &'static str = "local-storage::tasks_results-lock";

#[serde(crate = "alt_serde")]
#[derive(Encode, Decode, Deserialize, Clone)]
struct ErrandResultInfo {
    completed: bool,
    #[serde(deserialize_with = "de_string_to_bytes")]
    result_cid: Vec<u8>,
    failed_count: u32,
}

pub fn de_string_to_bytes<'de, D>(de: D) -> Result<Vec<u8>, D::Error>
where
    D: Deserializer<'de>,
{
    let s: &str = Deserialize::deserialize(de)?;
    Ok(s.as_bytes().to_vec())
}

pub mod crypto {
    use crate::KEY_TYPE;
    use sp_core::sr25519::Signature as Sr25519Signature;
    use sp_runtime::{
        app_crypto::{app_crypto, sr25519},
        traits::Verify,
        MultiSignature, MultiSigner,
    };

    app_crypto!(sr25519, KEY_TYPE);

    pub struct AuthId;

    // implemented for ocw-runtime
    impl frame_system::offchain::AppCrypto<MultiSigner, MultiSignature> for AuthId {
        type RuntimeAppPublic = Public;
        type GenericPublic = sp_core::sr25519::Public;
        type GenericSignature = sp_core::sr25519::Signature;
    }

    // implemented for mock runtime in test
    impl frame_system::offchain::AppCrypto<<Sr25519Signature as Verify>::Signer, Sr25519Signature>
        for AuthId
    {
        type RuntimeAppPublic = Public;
        type GenericPublic = sp_core::sr25519::Public;
        type GenericSignature = sp_core::sr25519::Signature;
    }
}

pub trait Trait: frame_system::Trait + CreateSignedTransaction<Call<Self>> {
    type Currency: ReservableCurrency<Self::AccountId>;
    type Event: From<Event<Self>> + Into<<Self as frame_system::Trait>::Event>;

    type AuthorityId: AppCrypto<Self::Public, Self::Signature>;
    type Call: From<Call<Self>>;
}

type ClientAccountId = Vec<u8>;

type ErrandId = Vec<u8>;

type Cid = Vec<u8>;

type NetAddress = Vec<u8>;

type DelegatorName = Vec<u8>;

#[derive(Encode, Decode, Clone, PartialEq, Eq, Debug)]
enum ErrandStatus {
    Processing,
    Done,
}

#[derive(Encode, Decode, Clone, PartialEq, Eq, Debug)]
pub struct Errand {
    account_id: ClientAccountId,
    errand_id: ErrandId,
    description_cid: Cid,
    status: ErrandStatus,
    result: Cid,
}

#[derive(Encode, Decode, Clone, PartialEq, Eq, Debug)]
pub struct TaskInfo {
    client: ClientAccountId,
    delegator: DelegatorName,
    description_cid: Cid,
    errand_id: ErrandId,
    fee: u32,
}

decl_storage! {
    trait Store for Module<T: Trait> as Abc {
        Errands get(fn errand):
            map hasher(twox_64_concat) Cid => Option<Errand>;

        Clients get(fn clients): map hasher(blake2_128_concat) T::AccountId => bool;

        ClientSender get(fn client_sender):
            map hasher(blake2_128_concat) T::AccountId => T::AccountId;

        ClientDelegateFee get(fn client_delegate_fee):
            map hasher(blake2_128_concat) T::AccountId => u32;

        ClientTaskFee get(fn client_task_fee):
            map hasher(blake2_128_concat) T::AccountId => u32;

        ClientNetAddress get(fn client_net_address):
            map hasher(blake2_128_concat) T::AccountId => NetAddress;

        Delegates get(fn client_delegator):
            map hasher(blake2_128_concat) DelegatorName => T::AccountId;

        Tasks get(fn tasks):
            map hasher(blake2_128_concat) T::BlockNumber => Vec<TaskInfo>;

        ClientsApplys get(fn delegate_accounts):
            map hasher(blake2_128_concat) T::BlockNumber => Vec<(T::AccountId, T::AccountId)>;

        ProcessingErrands get(fn processing_errands): Vec<Cid>;
    }
}

decl_event!(
    pub enum Event<T>
    where
        AccountId = <T as frame_system::Trait>::AccountId,
    {
        DelegateRequested(AccountId, AccountId),
        DelegateUpdated(AccountId),
        TaskBegan(AccountId, Vec<u8>, AccountId),
        ErrandInited(AccountId, Vec<u8>),
        ErrandUpdated(Vec<u8>, Vec<u8>),
    }
);

decl_error! {
    pub enum Error for Module<T: Trait> {
        NoneValue,
        StorageOverflow,
        InsufficientFee,
        SendErrandTaskError,
        QueryErrandResultError,
        ResponseParsingError,
        ErrandAlreadyExecuted,
        ErrandTaskNotExist,
        ErrandItemIsNone,
        ProcessingErrandNotExist,
        ApplyDelegateError,
        ClientAlreadyExists,
        ClientNotExist,
        ClientNotReady,
        ClientSenderNotExist,
        ClientDelegatorNotExist,
        NoRightToUpdateDelegate,
        NoRightToInitErrand,
        NoRightToUpdateErrand,
        AccountId32ConvertionError,
        LocalStorageError,
    }
}

decl_module! {
    pub struct Module<T: Trait> for enum Call where origin: T::Origin {
        type Error = Error<T>;

        fn deposit_event() = default;

        #[weight = 10_000]
        pub fn request_delegate(origin,
            client: T::AccountId,
            delegator_name: DelegatorName,
            net_address: NetAddress,
            fee: u32,
        ) -> dispatch::DispatchResult {
            let sender = ensure_signed(origin)?;

            ensure!(!Clients::<T>::contains_key(&client), Error::<T>::ClientAlreadyExists);

            // reserve fee for commit errand delegator
            ensure!(fee > 0, Error::<T>::InsufficientFee);
            T::Currency::reserve(&client, fee.into())?;
            ClientDelegateFee::<T>::insert(&client, fee);

            let block_number = frame_system::Module::<T>::block_number();
            if ClientsApplys::<T>::contains_key(&block_number) {
                let mut accounts = ClientsApplys::<T>::take(&block_number);
                accounts.push((client.clone(), sender.clone()));
                ClientsApplys::<T>::insert(&block_number, accounts);
            } else {
                ClientsApplys::<T>::insert(&block_number, vec![(client.clone(), sender.clone())]);
            }
            Clients::<T>::insert(&client, false);
            ClientSender::<T>::insert(&client, &sender);
            ClientNetAddress::<T>::insert(&client, net_address);
            Delegates::<T>::insert(delegator_name, &client);

            Self::deposit_event(RawEvent::DelegateRequested(client, sender));
            Ok(())
        }

        #[weight = 10_000]
        pub fn update_delegate_status(origin,
            client: T::AccountId,
            updater: T::AccountId,
        ) -> dispatch::DispatchResult {
            let sender = ensure_signed(origin)?;

            ensure!(sender == updater, Error::<T>::NoRightToUpdateDelegate);
            ensure!(Clients::<T>::contains_key(&client), Error::<T>::ClientNotExist);

            Clients::<T>::mutate(&client, |val| {
                *val = true;
            });
            Self::deposit_event(RawEvent::DelegateUpdated(client));
            Ok(())
        }

        #[weight = 10_000]
        pub fn begin_task(origin,
            client: T::AccountId,
            delegator_name: DelegatorName,
            description_cid: Cid,
            fee: u32,
            ) -> dispatch::DispatchResult {
            let sender = ensure_signed(origin)?;

            ensure!(Delegates::<T>::contains_key(&delegator_name), Error::<T>::ClientDelegatorNotExist);
            ensure!(Clients::<T>::contains_key(&Delegates::<T>::get(&delegator_name)), Error::<T>::ClientNotExist);
            ensure!(Clients::<T>::get(&Delegates::<T>::get(&delegator_name)), Error::<T>::ClientNotReady);
            ensure!(!Errands::contains_key(&description_cid), Error::<T>::ErrandAlreadyExecuted);

            let errand_id = Self::generate_errand_id(&sender);
            let task_info = TaskInfo {
                client: client.encode(),
                delegator: delegator_name,
                description_cid: description_cid.clone(),
                errand_id,
                fee,
            };

            let block_number = frame_system::Module::<T>::block_number();
            if Tasks::<T>::contains_key(&block_number) {
                let mut task_array = Tasks::<T>::take(&block_number);

                for task in task_array.iter() {
                    ensure!(!task.description_cid.eq(&description_cid), Error::<T>::ErrandAlreadyExecuted);
                }
                task_array.push(task_info);
                // reserve fee for commit errand delegator
                ensure!(fee > 0, Error::<T>::InsufficientFee);
                T::Currency::reserve(&client, fee.into())?;
                ClientTaskFee::<T>::insert(&client, fee);

                Tasks::<T>::insert(&block_number, task_array);
            } else {
                // reserve fee for commit errand delegator
                ensure!(fee > 0, Error::<T>::InsufficientFee);
                T::Currency::reserve(&client, fee.into())?;
                ClientTaskFee::<T>::insert(&client, fee);

                Tasks::<T>::insert(&block_number, vec![task_info]);
            }

            Self::deposit_event(RawEvent::TaskBegan(client, description_cid, sender));
            Ok(())
        }

        #[weight = 10_000]
        pub fn init_errand(origin,
            client: T::AccountId,
            errand_id: ErrandId,
            description_cid: Cid,
            ) -> dispatch::DispatchResult {

            let sender = ensure_signed(origin)?;
            ensure!(ClientSender::<T>::contains_key(&client), Error::<T>::ClientSenderNotExist);
            ensure!(sender == ClientSender::<T>::get(&client), Error::<T>::ClientSenderNotExist);

            let errand = Errand {
                account_id: client.encode(),
                errand_id: errand_id.clone(),
                description_cid: description_cid.clone(),
                status: ErrandStatus::Processing,
                result: Vec::new(),
            };
            Errands::insert(description_cid.clone(), errand);
            Self::add_processing(description_cid.clone());

            Self::deposit_event(RawEvent::ErrandInited(client, description_cid));
            Ok(())
        }

        #[weight = 10_000]
        pub fn update_errand(origin,
            description_cid: Cid,
            result: Vec<u8>,
            ) -> dispatch::DispatchResult {
            let sender = ensure_signed(origin)?;
            ensure!(Errands::contains_key(&description_cid), Error::<T>::ErrandTaskNotExist);
            let sender_account: AccountId32 = Self::account_to_bytes(&sender)?;
            let accounts: Vec<AccountId32> = vec![sender_account];

            #[cfg(feature = "std")]
            ensure!(task::account_from_seed_in_accounts("Alice", accounts), Error::<T>::NoRightToUpdateErrand);

            Errands::mutate(&description_cid, |val| {
                if let Some(errand) = val {
                    errand.status = ErrandStatus::Done;
                    errand.result = result.clone();
                }
            });
            Self::remove_processing(&description_cid);

            if let Some(errand) = Errands::get(&description_cid) {
                let client = Self::bytes_to_account(&mut errand.account_id.as_slice())?;
                let sender = ClientSender::<T>::get(&client);
                let fee = ClientTaskFee::<T>::get(&client);
                let balance = T::Currency::repatriate_reserved(&client, &sender, fee.into(), BalanceStatus::Free);
                match balance {
                    Ok(_b) => {debug::info!("repatriate reserved succeed");},
                    Err(_e) => {debug::error!("failed to repatriate reserved wiith cid: {:?}", description_cid);},
                }
            } else {
                debug::error!("found empty errand with cid: {:?}", description_cid);
            }

            Self::deposit_event(RawEvent::ErrandUpdated(description_cid, result));
            Ok(())
        }

        #[weight = 10_000]
        fn unreserve(origin,
            client: T::AccountId,
            fee: u32,
            ) {
            T::Currency::unreserve(&client, fee.into());
            Clients::<T>::remove(&client);
            ClientSender::<T>::remove(&client);
            ClientNetAddress::<T>::remove(&client);
        }

        fn offchain_worker(block_number: T::BlockNumber) {
            debug::info!("Entering off-chain workers");

            Self::apply_delegates(block_number);
            Self::send_errand_tasks(block_number);
            Self::query_errand_task_results(block_number);
            Self::update_errand_task_results(block_number);
        }
    }
}

impl<T: Trait> Module<T> {
    fn generate_errand_id(sender: &T::AccountId) -> Vec<u8> {
        let payload = (
            <pallet_randomness_collective_flip::Module<T> as Randomness<T::Hash>>::random_seed(),
            &sender,
            <frame_system::Module<T>>::extrinsic_index(),
        );
        let uuid = Builder::from_bytes(payload.using_encoded(blake2_128))
            .set_variant(Variant::RFC4122)
            .set_version(Version::Random)
            .build();
        let mut buf = Uuid::encode_buffer();
        let uuid = uuid.to_hyphenated().encode_lower(&mut buf);
        uuid.as_bytes().to_vec()
    }

    fn apply_delegates(block_number: T::BlockNumber) {
        if !ClientsApplys::<T>::contains_key(&block_number) {
            debug::info!("height {:?} has no delegates, just return", &block_number);
            return;
        }

        let signer = Signer::<T, T::AuthorityId>::all_accounts();
        if !signer.can_sign() {
            debug::info!("No local account available when apply delegate");
            return;
        }
        let account_ids: Vec<(T::AccountId, T::Public)> = Self::get_accounts();
        let accounts = ClientsApplys::<T>::get(&block_number);
        for acc in accounts.iter() {
            let mut signer_filter: Vec<T::Public> = Vec::new();
            for (aid, pk) in account_ids.iter() {
                if aid == &acc.1 {
                    signer_filter.push(pk.clone());
                }
            }
            if signer_filter.len() == 0 {
                continue;
            }
            let sender = ClientSender::<T>::get(&acc.0);
            let fee = ClientDelegateFee::<T>::get(&acc.0);

            if let Err(e) = Self::apply_single_delegate(&acc.0) {
                debug::error!("apply_single_delegate error: {:?}", e);
                // revert changes
                Signer::<T, T::AuthorityId>::all_accounts()
                    .with_filter(signer_filter)
                    .send_signed_transaction(|_acct| Call::unreserve(acc.0.clone(), fee.into()));
                continue;
            } else {
                let balance = T::Currency::repatriate_reserved(
                    &acc.0,
                    &sender,
                    fee.into(),
                    BalanceStatus::Free,
                );
                match balance {
                    Ok(_b) => {
                        debug::info!("repatriate reserved finished");
                    }
                    Err(e) => {
                        debug::error!("repatriate reserved: {:?}", e);
                    }
                }
            }
            let result = Signer::<T, T::AuthorityId>::all_accounts()
                .with_filter(signer_filter)
                .send_signed_transaction(|_acct| {
                    Call::update_delegate_status(acc.0.clone(), acc.1.clone())
                });

            for (_acc, err) in &result {
                if err.is_err() {
                    debug::error!("apply delegate {:?} error: {:?}", &acc.0, err);
                }
            }
        }
    }

    fn send_errand_tasks(block_number: T::BlockNumber) {
        if !Tasks::<T>::contains_key(&block_number) {
            debug::info!("height {:?} has no tasks, just return", &block_number);
            return;
        }

        let signer = Signer::<T, T::AuthorityId>::all_accounts();
        if !signer.can_sign() {
            debug::info!("No local account available when send errand tasks");
            return;
        }
        let account_ids: Vec<(T::AccountId, T::Public)> = Self::get_accounts();
        let task_array = Tasks::<T>::get(&block_number);
        for item in task_array.iter() {
            if !Delegates::<T>::contains_key(&item.delegator) {
                return;
            }
            let delegator = Delegates::<T>::get(&item.delegator);
            let sender = ClientSender::<T>::get(&delegator);
            let fee = ClientDelegateFee::<T>::get(&delegator);
            let mut signer_filter: Vec<T::Public> = Vec::new();
            for (aid, pk) in account_ids.iter() {
                if aid == &sender {
                    signer_filter.push(pk.clone());
                }
            }
            if signer_filter.len() == 0 {
                continue;
            }


            match Self::account_to_bytes(&delegator) {
                Ok(account) => {
                    let net_address = ClientNetAddress::<T>::get(&delegator);
                    #[cfg(feature = "std")]
                    if !task::send_task_to_tea_network(
                        &account,
                        &item.description_cid,
                        &item.errand_id,
                        &net_address,
                    ) {
                        T::Currency::unreserve(&delegator, fee.into());
                        continue;
                    }
                }
                Err(e) => debug::error!("decode client error: {:?}", e),
            }

            match T::AccountId::decode(&mut item.client.as_slice()) {
                Ok(client) => {
                    let selected_signer =
                        Signer::<T, T::AuthorityId>::all_accounts().with_filter(signer_filter);
                    if !Self::init_single_errand_task(
                        &selected_signer,
                        &client,
                        &item.description_cid,
                        &item.errand_id,
                    ) {
                        T::Currency::unreserve(&client, fee.into());
                    }
                }
                Err(e) => debug::error!("decode account id error: {:?}", e),
            }
        }
    }

    fn apply_single_delegate(delegator: &T::AccountId) -> Result<(), Error<T>> {
        let net_address = ClientNetAddress::<T>::get(&delegator);
        let account: AccountId32 = Self::account_to_bytes(&delegator)?;
        #[cfg(feature = "std")]
        if !delegate::request_single_delegate(account, &net_address) {
            return Err(Error::<T>::ApplyDelegateError);
        }

        Ok(())
    }

    fn query_errand_task_results(block_number: T::BlockNumber) {
        // query every 10 block (about 1 minute)
        let height: u64 = block_number.try_into().ok().unwrap() as u64;
        if height % 10 != 0 {
            return;
        }
        let signer = Signer::<T, T::AuthorityId>::all_accounts();
        if !signer.can_sign() {
            debug::info!("No local account available when query errand task results");
            return;
        }
        let accounts: Vec<AccountId32> = Self::get_account_ids();

        #[cfg(feature = "std")]
        if !task::account_from_seed_in_accounts("Alice", accounts) {
            return;
        }

        let processing_errands: Vec<Cid> = ProcessingErrands::get();
        debug::info!(
            "processing errand counts is {} at height {}",
            processing_errands.len(),
            height
        );
        for item in processing_errands {
            if let Some(errand) = Errands::get(&item) {
                match T::AccountId::decode(&mut errand.account_id.as_slice()) {
                    Ok(client) => {
                        let net_address = ClientNetAddress::<T>::get(&client);
                        #[cfg(feature = "std")]
                        task::fetch_single_task_result(
                            &errand.errand_id,
                            &errand.description_cid,
                            &net_address,
                        );
                    }
                    Err(e) => debug::error!("decode account id error: {:?}", e),
                }
            } else {
                debug::error!("found empty errand with cid: {:?}", item);
            }
        }
    }

    fn update_errand_task_results(_block_number: T::BlockNumber) {
        let signer = Signer::<T, T::AuthorityId>::all_accounts();
        if !signer.can_sign() {
            debug::info!("No local account available when update errand task results");
            return;
        }

        let accounts: Vec<AccountId32> = Self::get_account_ids();

        #[cfg(feature = "std")]
        if !task::account_from_seed_in_accounts("Alice", accounts) {
            return;
        }

        if let Err(e) = Self::load_tasks_results_info(&signer) {
            debug::error!("load_tasks_results_info error: {:?}", e);
        }
    }

    fn load_tasks_results_info(signer: &Signer<T, T::AuthorityId, ForAll>) -> Result<(), Error<T>> {
        let key = LOCAL_STORAGE_TASKS_RESULTS_KEY.as_bytes().to_vec();
        let lock_key = LOCAL_STORAGE_TASKS_RESULTS_LOCK.as_bytes().to_vec();

        let value_ref = StorageValueRef::persistent(&key);
        let lock = StorageValueRef::persistent(&lock_key);

        let res: Result<bool, bool> = lock.mutate(|s: Option<Option<bool>>| {
            match s {
                // `s` can be one of the following:
                //   `None`: the lock has never been set. Treated as the lock is free
                //   `Some(None)`: unexpected case, treated it as AlreadyFetch
                //   `Some(Some(false))`: the lock is free
                //   `Some(Some(true))`: the lock is held
                None | Some(Some(false)) => Ok(true),
                _ => Err(Error::<T>::LocalStorageError),
            }
        })?;

        match res {
            Ok(true) => {
                match value_ref.get::<Vec<(Cid, ErrandResultInfo)>>() {
                    Some(Some(results)) => {
                        for item in results.iter() {
                            Self::update_single_errand(signer, &item.1.result_cid, &item.0)?;
                        }

                        let empty_array: Vec<(Cid, ErrandResultInfo)> = vec![];
                        value_ref.set(&empty_array);
                    }
                    _ => {}
                }

                lock.set(&false);
                Ok(())
            }
            _ => Err(Error::<T>::LocalStorageError),
        }
    }

    fn update_single_errand(
        signer: &Signer<T, T::AuthorityId, ForAll>,
        result_cid: &Cid,
        description_cid: &Cid,
    ) -> Result<(), Error<T>> {
        let result = signer.send_signed_transaction(|_acct| {
            Call::update_errand(description_cid.clone(), result_cid.clone())
        });

        for (_acc, err) in &result {
            if err.is_err() {
                debug::error!(
                    "try update single errand {:?} error: {:?}",
                    description_cid,
                    err
                );
            }
        }
        Ok(())
    }

    fn init_single_errand_task(
        signer: &Signer<T, T::AuthorityId, ForAll>,
        client: &T::AccountId,
        description_cid: &Cid,
        errand_id: &ErrandId,
    ) -> bool {
        let result = signer.send_signed_transaction(|_acct| {
            Call::init_errand(client.clone(), errand_id.clone(), description_cid.clone())
        });

        for (_acc, err) in &result {
            if err.is_err() {
                debug::error!("init errand {:?} error: {:?}", errand_id, err);
                return false;
            }
        }
        true
    }

    fn add_processing(description_cid: Cid) {
        let mut errands: Vec<Cid> = ProcessingErrands::get();
        errands.push(description_cid);
        ProcessingErrands::put(errands);
    }

    fn remove_processing(description_cid: &Cid) {
        let mut errands: Vec<Cid> = ProcessingErrands::get();
        errands.retain(|item| !item.eq(description_cid));
        ProcessingErrands::put(errands);
    }

    fn account_to_bytes(account: &T::AccountId) -> Result<AccountId32, Error<T>> {
        let account_vec = account.encode();
        if account_vec.len() != 32 {
            return Err(Error::<T>::AccountId32ConvertionError);
        }
        let mut bytes = [0u8; 32];
        bytes.copy_from_slice(&account_vec);
        Ok(AccountId32::from(bytes))
    }

    fn bytes_to_account(mut account_bytes: &[u8]) -> Result<T::AccountId, Error<T>> {
        match T::AccountId::decode(&mut account_bytes) {
            Ok(client) => {
                return Ok(client);
            }
            Err(_e) => Err(Error::<T>::AccountId32ConvertionError),
        }
    }

    fn get_accounts() -> Vec<(T::AccountId, T::Public)> {
        let mut account_ids: Vec<(T::AccountId, T::Public)> = Vec::new();
        for (_pos, key) in
            <T::AuthorityId as AppCrypto<T::Public, T::Signature>>::RuntimeAppPublic::all()
                .into_iter()
                .enumerate()
        {
            let generic_public =
                <T::AuthorityId as AppCrypto<T::Public, T::Signature>>::GenericPublic::from(key);
            let public: T::Public = generic_public.into();
            let account_id: T::AccountId = public.clone().into_account();
            account_ids.push((account_id, public.clone()));
        }
        return account_ids;
    }

    fn get_account_ids() -> Vec<AccountId32> {
        let mut accounts: Vec<AccountId32> = Vec::new();
        for (_pos, key) in
            <T::AuthorityId as AppCrypto<T::Public, T::Signature>>::RuntimeAppPublic::all()
                .into_iter()
                .enumerate()
        {
            let generic_public =
                <T::AuthorityId as AppCrypto<T::Public, T::Signature>>::GenericPublic::from(key);
            let public = generic_public.into();
            let account_id = public.clone().into_account();
            match Self::account_to_bytes(&account_id) {
                Ok(account) => accounts.push(account),
                Err(e) => {
                    debug::error!("account_to_bytes convert {:?} error: {:?}", account_id, e);
                }
            }
        }
        return accounts;
    }
}
