#[derive(thiserror::Error, Debug)]
pub enum Error {
    #[error("Request failed")]
    RequestFailed,
}
