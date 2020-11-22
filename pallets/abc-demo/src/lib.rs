#![cfg_attr(not(feature = "std"), no_std)]

use codec::{Decode, Encode};
// todo enable ReservableCurrency later
// use frame_support::traits::ReservableCurrency;
use alt_serde::{Deserialize, Deserializer};
use core::convert::TryInto;
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
    // todo enable ReservableCurrency later
    // type Currency: ReservableCurrency<Self::AccountId>;
    type Event: From<Event<Self>> + Into<<Self as frame_system::Trait>::Event>;

    type AuthorityId: AppCrypto<Self::Public, Self::Signature>;
    type Call: From<Call<Self>>;
}

type EmployerAccountId = Vec<u8>;
type SenderAccountId = Vec<u8>;

type ErrandId = Vec<u8>;

type Cid = Vec<u8>;

#[derive(Encode, Decode, Clone, PartialEq, Eq, Debug)]
enum ErrandStatus {
    Processing,
    Done,
}

#[derive(Encode, Decode, Clone, PartialEq, Eq, Debug)]
pub struct Errand {
    account_id: EmployerAccountId,
    errand_id: ErrandId,
    description_cid: Cid,
    status: ErrandStatus,
    result: Cid,
}

#[derive(Encode, Decode, Clone, PartialEq, Eq, Debug)]
pub struct TaskInfo {
    employer: EmployerAccountId,
    sender: SenderAccountId,
    description_cid: Cid,
    errand_id: ErrandId,
    fee: u32,
}

decl_storage! {
    trait Store for Module<T: Trait> as Abc {
        Errands get(fn errand):
            map hasher(twox_64_concat) Cid => Option<Errand>;

        Employers get(fn employers): map hasher(blake2_128_concat) T::AccountId => bool;

        Tasks get(fn tasks):
            map hasher(blake2_128_concat) T::BlockNumber => Vec<TaskInfo>;

        EmployersApplys get(fn delegate_accounts):
            map hasher(blake2_128_concat) T::BlockNumber => Vec<(T::AccountId, T::AccountId)>;

        ProcessingErrands get(fn processing_errands): Vec<Cid>;
    }
}

decl_event!(
    pub enum Event<T>
    where
        AccountId = <T as frame_system::Trait>::AccountId,
    {
        // todo add events
        DelegateRequested(AccountId, AccountId),
        DelegateUpdated(AccountId),
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
        ApplyDelegateError,
        EmployerAlreadyExists,
        EmployerNotExist,
        EmployerNotReady,
        NoRightToUpdateDelegate,
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
            employer: T::AccountId,
        ) -> dispatch::DispatchResult {
            let sender = ensure_signed(origin)?;

            ensure!(!Employers::<T>::contains_key(&employer), Error::<T>::EmployerAlreadyExists);

            let block_number = frame_system::Module::<T>::block_number();
            if EmployersApplys::<T>::contains_key(&block_number) {
                let mut accounts = EmployersApplys::<T>::take(&block_number);
                accounts.push((employer.clone(), sender.clone()));
                EmployersApplys::<T>::insert(&block_number, accounts);
            } else {
                EmployersApplys::<T>::insert(&block_number, vec![(employer.clone(), sender.clone())]);
            }
            Employers::<T>::insert(&employer, false);

            Self::deposit_event(RawEvent::DelegateRequested(employer, sender));
            Ok(())
        }

        #[weight = 10_000]
        pub fn update_delegate_status(origin,
            employer: T::AccountId,
            updater: T::AccountId,
        ) -> dispatch::DispatchResult {
            let sender = ensure_signed(origin)?;

            ensure!(sender == updater, Error::<T>::NoRightToUpdateDelegate);
            ensure!(Employers::<T>::contains_key(&employer), Error::<T>::EmployerNotExist);

            Employers::<T>::mutate(&employer, |val| {
                *val = true;
            });
            Self::deposit_event(RawEvent::DelegateUpdated(employer));
            Ok(())
        }

        #[weight = 10_000]
        pub fn begin_task(origin,
            employer: T::AccountId,
            description_cid: Cid,
            fee: u32,
            ) -> dispatch::DispatchResult {
            let sender = ensure_signed(origin)?;

            // todo enable fee
            // reserve fee for commit errand delegator
            // ensure!(fee > 0, Error::<T>::InsufficientFee);
            // T::Currency::reserve(&sender, fee.into())?;

            ensure!(Employers::<T>::contains_key(&employer), Error::<T>::EmployerNotExist);
            ensure!(Employers::<T>::get(&employer), Error::<T>::EmployerNotReady);
            ensure!(!Errands::contains_key(&description_cid), Error::<T>::ErrandAlreadyExecuted);

            let errand_id = Self::generate_errand_id(&sender);
            let task_info = TaskInfo {
                employer: employer.encode(),
                sender: sender.encode(),
                description_cid: description_cid.clone(),
                errand_id,
                fee,
            };

            let block_number = frame_system::Module::<T>::block_number();
            if Tasks::<T>::contains_key(&block_number) {
                let mut task_array = Tasks::<T>::take(&block_number);

                for task in task_array.iter() {
                    ensure!(!Errands::contains_key(&task.description_cid), Error::<T>::ErrandAlreadyExecuted);
                }
                task_array.push(task_info);
                Tasks::<T>::insert(&block_number, task_array);
            } else {
                Tasks::<T>::insert(&block_number, vec![task_info]);
            }
            Self::add_processing(description_cid);

            Ok(())
        }

        #[weight = 10_000]
        pub fn init_errand(origin,
            employer: T::AccountId,
            errand_id: ErrandId,
            description_cid: Cid,
            ) -> dispatch::DispatchResult {

            let _sender = ensure_signed(origin)?;
            // todo ensure sender has right to init errand tasks

            let errand = Errand {
                account_id: employer.encode(),
                errand_id: errand_id.clone(),
                description_cid,
                status: ErrandStatus::Processing,
                result: Vec::new(),
            };
            Errands::insert(errand_id, errand);

            Ok(())
        }

        #[weight = 10_000]
        pub fn update_errand(origin,
            description_cid: Cid,
            result: Vec<u8>,
            ) -> dispatch::DispatchResult {
            let _sender = ensure_signed(origin)?;
            // todo ensure sender has right to init errand tasks

            ensure!(Errands::contains_key(&description_cid), Error::<T>::ErrandTaskNotExist);

            Errands::mutate(&description_cid, |val| {
                if let Some(errand) = val {
                    errand.status = ErrandStatus::Done;
                    errand.result = result;
                }
            });
            Self::remove_processing(&description_cid);

            Ok(())
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
        if !EmployersApplys::<T>::contains_key(&block_number) {
            debug::info!("height {:?} has no delegates, just return", &block_number);
            return;
        }

        let signer = Signer::<T, T::AuthorityId>::any_account();
        // if !signer.can_sign() {
        //     debug::info!("No local account available");
        //     return;
        // }

        let accounts = EmployersApplys::<T>::get(&block_number);
        for acc in accounts.iter() {
            // todo ensure signer has rights to init errand tasks
            if let Err(e) = Self::apply_single_delegate(&acc.0) {
                debug::error!("apply_single_delegate error: {:?}", e);
                continue;
            }
            let result = signer.send_signed_transaction(|_acct| {
                Call::update_delegate_status(acc.0.clone(), acc.1.clone())
            });

            for (_acc, err) in &result {
                debug::error!("apply delegate {:?} error: {:?}", &acc.0, err);
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
            debug::info!("No local account available");
            return;
        }
        // todo ensure signer has rights to init errand tasks
        let task_array = Tasks::<T>::get(&block_number);
        for item in task_array.iter() {
            match str::from_utf8(&item.employer) {
                Ok(employer) => {
                    #[cfg(feature = "std")]
                    if task::send_task_to_tea_network(
                        employer,
                        &item.description_cid,
                        &item.errand_id,
                    ) {
                        continue;
                    }

                    // todo decode sender from item.sender
                    let sender: T::AccountId = T::AccountId::default();
                    Self::init_single_errand_task(
                        &signer,
                        &sender,
                        &item.description_cid,
                        &item.errand_id,
                    );
                }
                Err(e) => debug::error!("convert employer to str error: {:?}", e),
            }
        }
    }

    fn apply_single_delegate(employer: &T::AccountId) -> Result<(), Error<T>> {
        let account: AccountId32 = Self::account_to_bytes(employer)?;
        #[cfg(feature = "std")]
        delegate::request_single_delegate(account);

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
            debug::info!("No local account available");
            return;
        }
        // todo ensure signer has rights to init errand tasks

        let processing_errands: Vec<Cid> = ProcessingErrands::get();
        for item in processing_errands {
            let errand: Errand = Errands::get(&item).unwrap();

            #[cfg(feature = "std")]
            task::fetch_single_task_result(&errand.errand_id, &errand.description_cid);
        }
    }

    fn update_errand_task_results(_block_number: T::BlockNumber) {
        let signer = Signer::<T, T::AuthorityId>::all_accounts();
        if !signer.can_sign() {
            debug::info!("No local account available");
            return;
        }
        // todo ensure signer has rights to init errand tasks

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
            debug::error!(
                "try update single errand {:?} error: {:?}",
                description_cid,
                err
            );
        }
        Ok(())
    }

    fn init_single_errand_task(
        signer: &Signer<T, T::AuthorityId, ForAll>,
        sender: &T::AccountId,
        description_cid: &Cid,
        errand_id: &ErrandId,
    ) {
        let result = signer.send_signed_transaction(|_acct| {
            Call::init_errand(sender.clone(), errand_id.clone(), description_cid.clone())
        });

        for (_acc, err) in &result {
            debug::error!("init errand {:?} error: {:?}", errand_id, err);
        }
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
}
