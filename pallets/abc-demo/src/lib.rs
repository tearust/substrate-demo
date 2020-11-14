#![cfg_attr(not(feature = "std"), no_std)]

use frame_system::ensure_signed;
use codec::{Decode, Encode};
use frame_support::{
	debug,
	decl_event, decl_module, decl_storage, decl_error, dispatch,
	StorageMap, StorageValue, IterableStorageMap, ensure,
	traits::{Randomness, Currency, ExistenceRequirement, Get, Imbalance}};
use sp_std::prelude::*;

#[cfg(test)]
mod mock;

#[cfg(test)]
mod tests;

pub trait Trait: frame_system::Trait {
	type Event: From<Event<Self>> + Into<<Self as frame_system::Trait>::Event>;
}

type EmployerAccountId = Vec<u8>;

type ErrandId = Vec<u8>;

type Cid = Vec<u8>;

#[derive(Encode, Decode, Clone, PartialEq, Eq, Debug)]
enum ErrandStatus {
	Precessing,
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

decl_storage! {
	trait Store for Module<T: Trait> as Abc {
		Errands get(fn errand):
			map hasher(twox_64_concat) ErrandId => Option<Errand>;
	}
}

decl_event!(
	pub enum Event<T>
	where
	AccountId = <T as frame_system::Trait>::AccountId {
		ErrandSended(AccountId, Errand),
	}
);

decl_error! {
	pub enum Error for Module<T: Trait> {
		NoneValue,
		StorageOverflow,
	}
}

decl_module! {
	pub struct Module<T: Trait> for enum Call where origin: T::Origin {
		type Error = Error<T>;

		fn deposit_event() = default;

		#[weight = 10_000]
		pub fn send_errand(origin,
			errand_id: ErrandId,
			description_cid: Cid) -> dispatch::DispatchResult {

			let sender = ensure_signed(origin)?;
			let errand = Errand {
				account_id: sender.encode(),
				errand_id: errand_id.clone(),
				description_cid,
				status: ErrandStatus::Precessing,
				result: Vec::new(),
			};
			Errands::insert(errand_id, errand);
			// Self::deposit_event(RawEvent::ErrandSended(sender, errand));

			Ok(())
		}

		fn offchain_worker(block_number: T::BlockNumber) {
			debug::info!("Entering off-chain workers");
		}
	}
}

impl<T: Trait> Module<T> {
	fn fetch_errand_execution_result() -> Result<Vec<u8>, Error<T>> {
		Ok(vec![])
	}
}