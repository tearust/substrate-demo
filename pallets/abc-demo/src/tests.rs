use super::*;
use crate::{mock::*, Error};
use frame_support::{assert_noop, assert_ok, debug};

#[test]
fn generate_errand_id_test() {
    new_test_ext().execute_with(|| {
        System::set_block_number(1);
        let errand_id = TemplateModule::generate_errand_id(&1u64);

        match String::from_utf8(errand_id) {
            Ok(uuid) => {
                assert_eq!(uuid, "278c4dc2-a301-4adc-ac12-1e2064df2e01");
            }
            Err(e) => panic!("error: {}", e),
        }
    });
}

#[test]
fn begin_task_works() {
    new_test_ext().execute_with(|| {
        // set block number to 1
        let block_number = 1;
        System::set_block_number(block_number);

        // add first task at height 1
        let cid = vec![1u8, 1u8];
        let fee = 5u32;
        let sender = Origin::signed(1);
        assert_ok!(TemplateModule::begin_task(sender.clone(), cid.clone(), fee));

        let task_array = Tasks::<Test>::get(&block_number);
        assert_eq!(1, task_array.len());
        assert_eq!(&1u64, &task_array[0].0);
        assert_eq!(&cid, &task_array[0].1);
        assert_eq!(&fee, &task_array[0].3);

        // add second task at height 1
        let cid = vec![1u8, 2u8];
        assert_ok!(TemplateModule::begin_task(sender.clone(), cid.clone(), fee));
        let task_array = Tasks::<Test>::get(&block_number);
        assert_eq!(2, task_array.len());
        assert_eq!(&cid, &task_array[1].1);

        // set block number to 2
        let block_number = 2;
        System::set_block_number(block_number);

        // add the same task as above
        assert_ok!(TemplateModule::begin_task(sender.clone(), cid.clone(), fee));
        let task_array2 = Tasks::<Test>::get(&block_number);
        assert_eq!(1, task_array2.len());
        assert_eq!(&1u64, &task_array2[0].0);
        assert_eq!(&cid, &task_array2[0].1);
        assert_eq!(&fee, &task_array2[0].3);
    })
}
