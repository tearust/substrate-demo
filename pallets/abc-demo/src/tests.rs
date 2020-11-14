use super::*;
use crate::{mock::*, Error};
use frame_support::{assert_noop, assert_ok, debug};

#[test]
fn it_works_for_default_value() {
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
