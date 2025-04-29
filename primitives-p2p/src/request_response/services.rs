use std::collections::HashMap;

/// The services request/response protocol name.
pub const PROTOCOL_NAME: &str = "/polka-storage/rr-services/1.0.0";
// NOTE(@jmg-duarte,10/04/2025): at the cost of extra dependencies
// (that we're already using in other crates) we could offer StreamProtocol here too

/// Information about a specific service.
#[derive(Debug, Clone)]
#[cfg_attr(feature = "serde", derive(::serde::Serialize, ::serde::Deserialize))]
pub struct ServiceInfo {
    pub port: u16,
    pub secure_url: Option<String>,
}

/// Holds a map between protocol names to service information
/// (e.g. `"ws": { port: 7008 }`).
#[derive(Debug, Clone)]
#[cfg_attr(feature = "serde", derive(::serde::Serialize, ::serde::Deserialize))]
pub struct Services(pub HashMap<String, ServiceInfo>);

/// A service request.
// We could try to support something like `type Request = struct All;`
// but this enables future extensions with minimal changes.
#[derive(Debug, Clone)]
#[cfg_attr(feature = "serde", derive(::serde::Serialize, ::serde::Deserialize))]
pub enum Request {
    // In any case, this will be serialized as `"All"` (at least under `serde_json` and `cbor4ii`).
    All,
}

/// A service response, containing the requested services or empty if none were found.
#[derive(Debug, Clone)]
#[cfg_attr(feature = "serde", derive(::serde::Serialize, ::serde::Deserialize))]
pub struct Response {
    pub services: Services,
}

/// Test for the shape of structures in this module.
///
/// Please refer to these tests when writing code interfacing with Javascript,
/// the wire-encoding format of the object itself *shouldn't* (maybe it will) matter.
#[cfg(all(test, feature = "serde"))]
mod tests {
    use std::collections::HashMap;

    use serde_json::{json, to_string};

    use super::ServiceInfo;
    use crate::services::{Request, Response, Services};

    #[test]
    fn service_info_shape() {
        assert_eq!(
            to_string(&ServiceInfo { port: 100, secure_url: None }).unwrap(),
            to_string(&json!({"port": 100})).unwrap()
        )
    }

    #[test]
    fn services_shape() {
        assert_eq!(
            to_string(&Services({
                let mut h = HashMap::new();
                h.insert("ws".to_string(), ServiceInfo { port: 1000, secure_url: None });
                h
            }))
            .unwrap(),
            to_string(&json!({
                "ws": {
                    "port": 1000
                }
            }))
            .unwrap()
        )
    }

    #[test]
    fn request_shape() {
        assert_eq!(
            to_string(&Request::All).unwrap(),
            to_string(&json!("All")).unwrap()
        );
    }

    #[test]
    fn response_shape() {
        assert_eq!(
            to_string(&Response {
                services: Services(HashMap::new())
            })
            .unwrap(),
            to_string(&json!({"services": {}})).unwrap()
        );

        assert_eq!(
            to_string(&Response {
                services: Services({
                    let mut h = HashMap::new();
                    h.insert("ws".to_string(), ServiceInfo { port: 1000, secure_url: None });
                    h
                })
            })
            .unwrap(),
            to_string(&json!({
                "services": {
                    "ws": {
                        "port": 1000
                    }
                }
            }))
            .unwrap()
        );
    }
}
