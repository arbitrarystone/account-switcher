//! 账号领域：元数据模型、持久化、业务服务。

mod error;
mod model;
mod service;
mod store;

pub use error::AccountError;
pub use model::{Account, AccountUpdate, NewAccount, Tool};
pub use service::AccountService;
pub use store::JsonFileStore;
