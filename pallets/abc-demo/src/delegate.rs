use crate::error::AbcError;
use crate::storage::operate_local_storage;
use crate::{de_string_to_bytes, NetAddress, SERVICE_BASE_URL_PREFIX};
use alt_serde::Deserialize;
use codec::{Decode, Encode};
use frame_support::debug;
use sp_core::crypto::AccountId32;

const APPLY_DELEGATE: &'static str = "/api/be_my_delegate";

const LOCAL_STORAGE_CLIENT_KEY_PREFIX: &'static str = "local-storage::client-";
const LOCAL_STORAGE_CLIENT_LOCK_PREFIX: &'static str = "local-storage::client-lock-";

mod actor_delegate_proto {
    include!(concat!(env!("OUT_DIR"), "/actor_delegate.rs"));
}

#[serde(crate = "alt_serde")]
#[derive(Encode, Decode, Deserialize)]
pub struct DelegateInfo {
    #[serde(deserialize_with = "de_string_to_bytes")]
    pub delegator_tea_id: Vec<u8>,
    #[serde(deserialize_with = "de_string_to_bytes")]
    pub delegator_ephemeral_id: Vec<u8>,
    #[serde(deserialize_with = "de_string_to_bytes")]
    pub sig: Vec<u8>,

    pub key3_rsa_pub_key: String,
}

pub fn get_url(net_address: &NetAddress) -> anyhow::Result<String> {
    let mut url = String::new();
    url.push_str(SERVICE_BASE_URL_PREFIX);
    let ip_str = String::from_utf8(net_address.to_vec());
    match ip_str {
        Ok(str) => {
            url.push_str(&str);
            Ok(url)
        }
        Err(e) => Err(anyhow::anyhow!(
            "get url from net address failed, error:{}",
            e
        )),
    }
}

pub fn request_single_delegate(account: AccountId32, net_address: &NetAddress) -> bool {
    let client = format!("{}", account);
    let proto_msg = actor_delegate_proto::BeMyDelegateRequest {
        layer1_account: client.clone(),
        // todo generate a nonce value
        nonce: Vec::<u8>::new(),
    };
    let service_url: String;
    let url = get_url(net_address);
    match url {
        Ok(u) => service_url = u,
        Err(e) => {
            debug::error!("delegate request failed: {}", e);
            return false;
        }
    }
    match encode_protobuf(proto_msg) {
        Ok(buf) => {
            let content = base64::encode(buf);
            let request_url = format!("{}{}?content={}", service_url, APPLY_DELEGATE, &content);

            match crate::http::http_post(&request_url) {
                Ok(resp) => {
                    if let Err(e) = parse_delegate_response(resp, &client) {
                        debug::error!("parse delegate response: {}", e);
                        return false;
                    }
                }
                Err(e) => {
                    debug::error!("delegate request failed: {}", e);
                    return false;
                }
            }
        }
        Err(e) => {
            debug::error!("encode protobuf failed: {}", e);
            return false;
        }
    }
    true
}

fn parse_delegate_response(resp: Vec<u8>, client: &str) -> anyhow::Result<()> {
    let resp_str = String::from_utf8(resp)?;
    let result_info: DelegateInfo = serde_json::from_str::<DelegateInfo>(&resp_str)
        .map_err(|e| AbcError::Common(format!("{}", e)))?;

    save_delegate_info(client, &result_info)
}

fn save_delegate_info(client: &str, delegate_info: &DelegateInfo) -> anyhow::Result<()> {
    let key = [LOCAL_STORAGE_CLIENT_KEY_PREFIX, client]
        .concat()
        .as_bytes()
        .to_vec();
    let lock_key = [LOCAL_STORAGE_CLIENT_LOCK_PREFIX, client]
        .concat()
        .as_bytes()
        .to_vec();
    operate_local_storage(key, lock_key, |value_ref| {
        value_ref.set(delegate_info);
        ()
    })
}

pub fn load_delegate_info(client: &str) -> anyhow::Result<DelegateInfo> {
    let key = [LOCAL_STORAGE_CLIENT_KEY_PREFIX, client]
        .concat()
        .as_bytes()
        .to_vec();
    let lock_key = [LOCAL_STORAGE_CLIENT_LOCK_PREFIX, client]
        .concat()
        .as_bytes()
        .to_vec();
    match operate_local_storage(key, lock_key, |value_ref| value_ref.get::<DelegateInfo>()) {
        Ok(Some(Some(info))) => Ok(info),
        Err(e) => Err(anyhow::anyhow!(
            "get local storage about {} error, details: {}",
            client,
            e
        )),
        _ => Err(anyhow::anyhow!("get local storage about {} error", client)),
    }
}

pub fn encode_protobuf<T>(protobuf_type: T) -> anyhow::Result<Vec<u8>>
where
    T: prost::Message,
{
    let mut buf: Vec<u8> = Vec::with_capacity(protobuf_type.encoded_len());
    protobuf_type.encode(&mut buf)?;
    Ok(buf)
}
