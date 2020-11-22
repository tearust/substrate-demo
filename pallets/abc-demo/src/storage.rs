use sp_runtime::offchain::storage::StorageValueRef;

pub fn operate_local_storage<F, R>(
    key: Vec<u8>,
    lock_key: Vec<u8>,
    mut callback: F,
) -> anyhow::Result<R>
where
    F: FnMut(StorageValueRef) -> R,
{
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
            _ => Err(anyhow::anyhow!(
                "local storage key {} locked",
                String::from_utf8(key.clone())?
            )),
        }
    })?;

    match res {
        Ok(true) => {
            let rtn = callback(value_ref);
            lock.set(&false);
            Ok(rtn)
        }
        _ => Err(anyhow::anyhow!(
            "local storage key {} lock error",
            String::from_utf8(key)?
        )),
    }
}
