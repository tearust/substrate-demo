use super::*;
use crate::mock::*;
use frame_support::assert_ok;
use sp_core::crypto::{AccountId32, Ss58Codec};

pub const ACCOUNT1: &str = "5G97JLuuT1opraWvfS6Smt4jaAZuyDquP9GjamKVcPC366qU";
pub const ACCOUNT2: &str = "5EPhGXBymJfrcvT2apo6E9tAm2xpNf8Y77zLxd4zjCBdVJeS";

#[test]
fn generate_errand_id_test() {
    new_test_ext().execute_with(|| {
        System::set_block_number(1);
        let errand_id =
            TemplateModule::generate_errand_id(&AccountId32::from_string(ACCOUNT1).unwrap());
        match String::from_utf8(errand_id) {
            Ok(uuid) => {
                assert_eq!(uuid, "a1aa04f9-adc0-487a-ab5d-ba1ddb9b963b");
            }
            Err(e) => panic!("error: {}", e),
        }
    });
}

#[test]
fn begin_task_works() {
    let a = vec![1];
    for _i in a.iter() {}
    new_test_ext().execute_with(|| {
        // set block number to 1
        let block_number = 1;
        System::set_block_number(block_number);

        // add first task at height 1
        let cid = vec![1u8, 1u8];
        let fee = 5u32;
        let sender = Origin::signed(AccountId32::from_string(ACCOUNT1).unwrap());
        let sender_account = AccountId32::from_string(ACCOUNT1).unwrap();
        let client_account = AccountId32::from_string(ACCOUNT2).unwrap();
        assert_ok!(TemplateModule::request_delegate(
            sender.clone(),
            client_account.clone(),
            vec![0u8],
            fee.into()
        ));
        assert_ok!(TemplateModule::update_delegate_status(
            sender.clone(),
            sender_account.clone()
        ));
        assert_ok!(TemplateModule::begin_task(
            sender.clone(),
            client_account.clone(),
            cid.clone(),
            fee
        ));

        let task_array = Tasks::<Test>::get(&block_number);
        let mut sender_bytes = [0u8; 32];
        sender_bytes.copy_from_slice(task_array[0].sender.as_slice());
        let mut client_bytes = [0u8; 32];
        client_bytes.copy_from_slice(task_array[0].client.as_slice());
        assert_eq!(1, task_array.len());
        assert_eq!(
            &AccountId32::from_string(ACCOUNT1).unwrap(),
            &AccountId32::from(sender_bytes)
        );
        assert_eq!(
            &AccountId32::from_string(ACCOUNT2).unwrap(),
            &AccountId32::from(client_bytes)
        );
        assert_eq!(&fee, &task_array[0].fee);

        // add second task at height 1
        let cid = vec![1u8, 2u8];
        assert_ok!(TemplateModule::begin_task(
            sender.clone(),
            client_account.clone(),
            cid.clone(),
            fee
        ));
        let task_array = Tasks::<Test>::get(&block_number);
        assert_eq!(2, task_array.len());
        assert_eq!(&cid, &task_array[1].description_cid);

        // set block number to 2
        let block_number = 2;
        System::set_block_number(block_number);

        // add the same task as above
        assert_ok!(TemplateModule::begin_task(
            sender.clone(),
            client_account.clone(),
            cid.clone(),
            fee
        ));
        let task_array2 = Tasks::<Test>::get(&block_number);
        let mut sender_bytes_2 = [0u8; 32];
        sender_bytes_2.copy_from_slice(task_array2[0].sender.as_slice());
        assert_eq!(1, task_array2.len());
        assert_eq!(
            &AccountId32::from_string(ACCOUNT1).unwrap(),
            &AccountId32::from(sender_bytes_2)
        );
        assert_eq!(&cid, &task_array2[0].description_cid);
        assert_eq!(&fee, &task_array2[0].fee);

        let errand_id = vec![3u8, 4u8];
        assert_ok!(TemplateModule::init_errand(
            sender.clone(),
            client_account.clone(),
            errand_id.clone(),
            cid.clone()
        ));
        if let Some(errand) = Errands::get(&cid.clone()) {
            assert_eq!(&errand.errand_id, &errand_id.clone());
        } else {
            assert_eq!(false, true);
        }
    })
}
