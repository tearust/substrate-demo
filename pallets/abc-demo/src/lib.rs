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
use sp_core::crypto::KeyTypeId;
use sp_io::hashing::blake2_128;
use sp_runtime::offchain::{self as rt_offchain, storage::StorageValueRef};
use sp_std::prelude::*;
use sp_std::str;
use uuid::{Builder, Uuid, Variant, Version};

#[cfg(test)]
mod mock;

#[cfg(test)]
mod tests;

pub const SERVICE_BASE_URL: &'static str = "http://localhost:8000";
pub const SEND_ERRAND_TASK_ACTION: &'static str = "/api/service";
pub const QUERY_ERRAND_RESULT_ACTION: &'static str = "/api/query_errand_execution_result_by_uuid";
pub const APPLY_DELEGATE: &'static str = "/api/be_my_delegate";

pub const KEY_TYPE: KeyTypeId = KeyTypeId(*b"demo");
pub const TEA_SEND_TASK_TIMEOUT_PERIOD: u64 = 3000;

pub const LOCAL_STORAGE_EMPLOYER_KEY_PREFIX: &'static str = "local-storage::employer-";
pub const LOCAL_STORAGE_EMPLOYER_LOCK_PREFIX: &'static str = "local-storage::employer-lock-";

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

#[serde(crate = "alt_serde")]
#[derive(Encode, Decode, Deserialize)]
struct ErrandResultInfo {
    completed: bool,
    #[serde(deserialize_with = "de_string_to_bytes")]
    result_cid: Vec<u8>,
    failed_count: u32,
}

#[serde(crate = "alt_serde")]
#[derive(Encode, Decode, Deserialize)]
struct DelegateInfo {
    #[serde(deserialize_with = "de_string_to_bytes")]
    delegator_tea_id: Vec<u8>,
    #[serde(deserialize_with = "de_string_to_bytes")]
    delegator_ephemeral_id: Vec<u8>,
    #[serde(deserialize_with = "de_string_to_bytes")]
    sig: Vec<u8>,
    #[serde(deserialize_with = "de_string_to_bytes")]
    key3_rsa_pub_key: Vec<u8>,
}

pub fn de_string_to_bytes<'de, D>(de: D) -> Result<Vec<u8>, D::Error>
where
    D: Deserializer<'de>,
{
    let s: &str = Deserialize::deserialize(de)?;
    Ok(s.as_bytes().to_vec())
}

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
    result: Vec<u8>,
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
        ErrandSended(AccountId, Errand),
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
        LocalStorageError,
        LocalStorageAlreadyFetched,
        EmployerAlreadyExists,
        EmployerNotExist,
        EmployerNotReady,
        NoRightToUpdateDelegate,
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
                accounts.push((employer.clone(), sender));
                EmployersApplys::<T>::insert(&block_number, accounts);
            } else {
                EmployersApplys::<T>::insert(&block_number, vec![(employer.clone(), sender)]);
            }
            Employers::<T>::insert(&employer, false);

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
            // Self::deposit_event(RawEvent::ErrandSended(sender, errand));

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

            Self::apply_delegates();
            Self::send_errand_tasks();
            Self::query_errand_task_results(block_number);
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

    fn apply_delegates() {
        let current_height = frame_system::Module::<T>::block_number();
        if !Tasks::<T>::contains_key(&current_height) {
            debug::info!("height {:?} has no delegates, just return", &current_height);
            return;
        }

        let signer = Signer::<T, T::AuthorityId>::all_accounts();
        if !signer.can_sign() {
            debug::info!("No local account available");
            return;
        }

        let accounts = EmployersApplys::<T>::get(&current_height);
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

    fn send_errand_tasks() {
        let current_height = frame_system::Module::<T>::block_number();
        if !Tasks::<T>::contains_key(&current_height) {
            debug::info!("height {:?} has no tasks, just return", &current_height);
            return;
        }

        let signer = Signer::<T, T::AuthorityId>::all_accounts();
        if !signer.can_sign() {
            debug::info!("No local account available");
            return;
        }
        // todo ensure signer has rights to init errand tasks
        let task_array = Tasks::<T>::get(&current_height);
        for item in task_array.iter() {
            match str::from_utf8(&item.employer) {
                Ok(employer) => {
                    if let Err(e) = Self::send_task_to_tea_network(
                        employer,
                        &item.description_cid,
                        &item.errand_id,
                    ) {
                        debug::error!("send_task_to_tea_network error: {:?}", e);
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
        let request_url = [
            SERVICE_BASE_URL,
            APPLY_DELEGATE,
            "?content=",
            str::from_utf8(&employer.encode()).map_err(|_| Error::<T>::ApplyDelegateError)?,
        ]
        .concat();

        let resp = Self::http_post(&request_url)?;
        let resp_str = str::from_utf8(&resp).map_err(|_| Error::<T>::ResponseParsingError)?;
        let result_info: DelegateInfo = serde_json::from_str::<DelegateInfo>(resp_str)
            .map_err(|_| Error::<T>::ResponseParsingError)?;

        Self::save_delegate_info(
            str::from_utf8(&employer.encode()).map_err(|_| Error::<T>::ApplyDelegateError)?,
            &result_info,
        )?;

        Ok(())
    }

    fn operate_local_storage<F, R>(employer: &str, mut callback: F) -> Result<R, Error<T>>
    where
        F: FnMut(StorageValueRef) -> R,
    {
        let key = [LOCAL_STORAGE_EMPLOYER_KEY_PREFIX, employer]
            .concat()
            .as_bytes()
            .to_vec();
        let lock_key = [LOCAL_STORAGE_EMPLOYER_LOCK_PREFIX, employer]
            .concat()
            .as_bytes()
            .to_vec();
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
                _ => Err(Error::<T>::LocalStorageAlreadyFetched),
            }
        })?;

        match res {
            Ok(true) => {
                let rtn = callback(value_ref);
                lock.set(&false);
                Ok(rtn)
            }
            _ => Err(Error::<T>::LocalStorageError),
        }
    }

    fn save_delegate_info(employer: &str, delegate_info: &DelegateInfo) -> Result<(), Error<T>> {
        Self::operate_local_storage(employer, |value_ref| {
            value_ref.set(delegate_info);
            ()
        })
    }

    fn load_delegate_info(employer: &str) -> Result<DelegateInfo, Error<T>> {
        match Self::operate_local_storage(employer, |value_ref| value_ref.get::<DelegateInfo>()) {
            Ok(Some(Some(info))) => Ok(info),
            _ => Err(Error::<T>::LocalStorageError),
        }
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
            if let Err(e) = Self::try_update_single_errand(&signer, &errand.errand_id, &item) {
                debug::error!("try_update_single_errand execute error: {:?}", e);
            }
        }
    }

    fn try_update_single_errand(
        signer: &Signer<T, T::AuthorityId, ForAll>,
        errand_id: &ErrandId,
        description_cid: &Cid,
    ) -> Result<(), Error<T>> {
        let resp_bytes = Self::query_single_task_result(errand_id).map_err(|e| {
            debug::error!("query_result_from_http error: {:?}", e);
            Error::<T>::QueryErrandResultError
        })?;

        let resp_str = str::from_utf8(&resp_bytes).map_err(|_| Error::<T>::ResponseParsingError)?;
        let result_info: ErrandResultInfo = serde_json::from_str::<ErrandResultInfo>(resp_str)
            .map_err(|_| Error::<T>::ResponseParsingError)?;

        if !result_info.completed {
            return Ok(());
        }

        let result = signer.send_signed_transaction(|_acct| {
            Call::update_errand(description_cid.clone(), result_info.result_cid.clone())
        });

        for (_acc, err) in &result {
            debug::error!("try update single errand {:?} error: {:?}", errand_id, err);
        }
        Ok(())
    }

    fn query_single_task_result(errand_id: &ErrandId) -> Result<Vec<u8>, Error<T>> {
        let request_url = [
            SERVICE_BASE_URL,
            QUERY_ERRAND_RESULT_ACTION,
            "/",
            str::from_utf8(&errand_id).map_err(|_| Error::<T>::QueryErrandResultError)?,
        ]
        .concat();
        Self::http_post(&request_url)
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

    fn send_task_to_tea_network(
        employer: &str,
        description_cid: &Cid,
        errand_id: &ErrandId,
    ) -> Result<(), Error<T>> {
        let info: DelegateInfo = Self::load_delegate_info(employer)?;
        let request_url = [
            SERVICE_BASE_URL,
            SEND_ERRAND_TASK_ACTION,
            "/",
            employer,
            "/",
            str::from_utf8(&errand_id).map_err(|_| Error::<T>::SendErrandTaskError)?,
            "/",
            // todo convert signature to hex string
            str::from_utf8(&info.sig).map_err(|_| Error::<T>::SendErrandTaskError)?,
            "?content=",
            str::from_utf8(&description_cid).map_err(|_| Error::<T>::SendErrandTaskError)?,
        ]
        .concat();
        Self::http_post(&request_url)?;

        Ok(())
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

    fn http_post(url: &str) -> Result<Vec<u8>, Error<T>> {
        let post_body = vec![b""];

        let request = rt_offchain::http::Request::post(url, post_body);
        let timeout = sp_io::offchain::timestamp().add(rt_offchain::Duration::from_millis(3000));
        let pending = request
            .deadline(timeout)
            .send()
            .map_err(|_| Error::<T>::SendErrandTaskError)?;

        let response = pending
            .try_wait(timeout)
            .map_err(|_| Error::<T>::SendErrandTaskError)?
            .map_err(|_| Error::<T>::SendErrandTaskError)?;

        if response.code != 200 {
            debug::error!("Unexpected http request status code: {}", response.code);
            return Err(<Error<T>>::SendErrandTaskError);
        }

        Ok(response.body().collect::<Vec<u8>>())
    }
}
