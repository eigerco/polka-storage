use std::collections::HashMap;

pub const PROTOCOL_NAME: &str = "/polka-storage/rr-services/1.0.0";

#[derive(Debug, Clone)]
#[cfg_attr(feature = "serde", derive(::serde::Serialize, ::serde::Deserialize))]
pub struct ServiceInfo {
    pub port: u16,
}

/// Holds a map between protocol names to service information
/// (e.g. `"ws": { port: 7008 }`).
#[derive(Debug, Clone)]
#[cfg_attr(feature = "serde", derive(::serde::Serialize, ::serde::Deserialize))]
pub struct Services(pub HashMap<String, ServiceInfo>);

impl Services {
    pub fn get(&self, service: &String) -> Services {
        Services(match self.0.get(service) {
            Some(info) => {
                let mut m = HashMap::new();
                m.insert(service.clone(), info.clone());
                m
            }
            None => HashMap::new(),
        })
    }

    pub fn get_n<'s, I>(&self, services: I) -> Services
    where
        I: Iterator<Item = &'s String>,
    {
        let mut m = HashMap::new();
        for service in services {
            if let Some(info) = self.0.get(service) {
                m.insert(service.clone(), info.clone());
            }
        }
        Services(m)
    }
}

/// A service request, supports requesting information for
/// single, multiple and all protocols available.
#[derive(Debug, Clone)]
#[cfg_attr(feature = "serde", derive(::serde::Serialize, ::serde::Deserialize))]
pub enum Request {
    Specific(String),
    Multiple(Vec<String>),
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
            to_string(&ServiceInfo { port: 100 }).unwrap(),
            to_string(&json!({"port": 100})).unwrap()
        )
    }

    #[test]
    fn services_shape() {
        assert_eq!(
            to_string(&Services({
                let mut h = HashMap::new();
                h.insert("ws".to_string(), ServiceInfo { port: 1000 });
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
            to_string(&Request::Specific("ws".to_string())).unwrap(),
            to_string(&json!({"Specific": "ws"})).unwrap()
        );

        assert_eq!(
            to_string(&Request::Multiple(vec![
                "ws".to_string(),
                "http".to_string()
            ]))
            .unwrap(),
            to_string(&json!({"Multiple": ["ws", "http"]})).unwrap()
        );

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
                    h.insert("ws".to_string(), ServiceInfo { port: 1000 });
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
