#![cfg_attr(not(feature = "std"), no_std)]

use frame_system::ensure_signed;
use codec::{Decode, Encode};
use frame_support::{
	debug,
	decl_event, decl_module, decl_storage, decl_error, dispatch,
	StorageMap, StorageValue, IterableStorageMap, ensure,
	traits::{Randomness, Currency, ExistenceRequirement, Get, Imbalance}};

#[cfg(test)]
mod mock;

#[cfg(test)]
mod tests;

pub trait Trait: frame_system::Trait {
	type Event: From<Event<Self>> + Into<<Self as frame_system::Trait>::Event>;
}

type ErrandId = Vec<u8>;

type Cid = Vec<u8>;

#[derive(Encode, Decode, Default, Clone, PartialEq, Eq, Debug)]
enum ErrandStatus {
	Precessing,
	Done,
}

#[derive(Encode, Decode, Default, Clone, PartialEq, Eq, Debug)]
pub struct Errand<AccountId> {
	account_id: AccountId,
	errand_id: ErrandId,
	description_cid: Cid,
	status: ErrandStatus,
	result: Vec<u8>,
}

decl_storage! {
	trait Store for Module<T: Trait> as Abc {
		Errands get(fn errand):
			map hasher(twox_64_concat) ErrandId => Option<Errand<T::AccountId>>;
	}
}

decl_event!(
	pub enum Event<T>
	where
	AccountId = <T as frame_system::Trait>::AccountId {
		ErrandSended(AccountId, Errand<AccountId>),
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
			let errand = Errand{
				account_id: &sender,
				errand_id,
				description_cid,
				status: ErrandStatus::Precessing,
				result: Vec::new(),
			};
			Errands::<T>::insert(errand_id, errand);
			// Self::deposit_event(RawEvent::ErrandSended(sender, errand));

			Ok(())
		}
	}
}
