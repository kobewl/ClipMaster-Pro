//! macOS 系统钥匙串实现：Security.framework 的 generic password 条目。
//!
//! 为什么用 generic password 而不是自己加密一个文件：钥匙串由系统把关 ——
//! 条目按「谁写的」绑定 ACL，换了可执行文件签名就取不出来，磁盘上的内容
//! 也由系统密钥保护。自己拿固定密钥加密只是把钥匙换个地方藏，属于安全表演。
//!
//! 所有调用都走 `spawn_blocking`：Security.framework 是同步阻塞 API，
//! 而且**首次读取会弹系统授权框**，可能停在那里等用户点，绝不能占住 async 线程。

use async_trait::async_trait;
use security_framework::passwords::{
    delete_generic_password, generic_password, set_generic_password, PasswordOptions,
};

use crate::domain::error::SecretError;
use crate::domain::ports::SecretStore;

/// `errSecItemNotFound`：钥匙串里没有这一项。
///
/// 直接比较状态码而不是再引入 `security-framework-sys` —— 这些常量是
/// Security.framework 的稳定 ABI 值，不随系统版本变化。
const ERR_SEC_ITEM_NOT_FOUND: i32 = -25300;
/// 认证失败 / 用户点了「拒绝」。
const ERR_SEC_AUTH_FAILED: i32 = -25293;
/// 用户取消了那次授权弹窗。
const ERR_SEC_USER_CANCELED: i32 = -128;
/// 当前不允许弹授权框（例如屏幕锁定中）。
const ERR_SEC_INTERACTION_NOT_ALLOWED: i32 = -25308;

/// 钥匙串条目的 service 名：与 `tauri.conf.json` 里的 bundle id 保持一致，
/// 用户在「钥匙串访问」里看到它就知道是哪个应用存的。
const SERVICE: &str = "pro.clipmaster.desktop";

pub struct KeychainSecretStore;

impl KeychainSecretStore {
    pub fn new() -> Self {
        Self
    }
}

impl Default for KeychainSecretStore {
    fn default() -> Self {
        Self::new()
    }
}

/// 把 Security.framework 的错误转成领域错误。
///
/// 只保留状态码，不带 `message()` —— 系统返回的描述里可能带上条目属性，
/// 而这些函数处理的就是密钥本身，宁可不放进日志。
///
/// 授权类错误单独给一句人话：应用重新编译/覆盖安装后签名会变，
/// 系统会重新弹一次「允许读取钥匙串吗」，用户点错就会撞到这里，
/// 报"状态码 -25293"对他没有任何帮助。
fn map_err(err: security_framework::base::Error) -> SecretError {
    let hint = match err.code() {
        ERR_SEC_AUTH_FAILED => "（钥匙串授权被拒绝，请在「钥匙串访问」里允许 ClipMaster Pro 读取）",
        ERR_SEC_USER_CANCELED => "（你取消了钥匙串授权弹窗）",
        ERR_SEC_INTERACTION_NOT_ALLOWED => "（当前无法弹出钥匙串授权框，请解锁屏幕后重试）",
        _ => "",
    };
    SecretError::Access(format!(
        "Security.framework 状态码 {}{hint}",
        err.code()
    ))
}

#[async_trait]
impl SecretStore for KeychainSecretStore {
    async fn get(&self, account: &str) -> Result<Option<String>, SecretError> {
        let account = account.to_string();
        tokio::task::spawn_blocking(move || {
            let options = PasswordOptions::new_generic_password(SERVICE, &account);
            match generic_password(options) {
                Ok(bytes) => String::from_utf8(bytes).map(Some).map_err(|_| {
                    SecretError::Access("钥匙串条目不是合法 UTF-8".to_string())
                }),
                Err(err) if err.code() == ERR_SEC_ITEM_NOT_FOUND => Ok(None),
                Err(err) => Err(map_err(err)),
            }
        })
        .await
        .map_err(|err| SecretError::Unavailable(err.to_string()))?
    }

    async fn set(&self, account: &str, value: &str) -> Result<(), SecretError> {
        let account = account.to_string();
        let value = value.as_bytes().to_vec();
        tokio::task::spawn_blocking(move || {
            set_generic_password(SERVICE, &account, &value).map_err(map_err)
        })
        .await
        .map_err(|err| SecretError::Unavailable(err.to_string()))?
    }

    async fn delete(&self, account: &str) -> Result<(), SecretError> {
        let account = account.to_string();
        tokio::task::spawn_blocking(move || match delete_generic_password(SERVICE, &account) {
            Ok(()) => Ok(()),
            // 本来就没了也算删干净：设置界面反复点"清除"不该报错。
            Err(err) if err.code() == ERR_SEC_ITEM_NOT_FOUND => Ok(()),
            Err(err) => Err(map_err(err)),
        })
        .await
        .map_err(|err| SecretError::Unavailable(err.to_string()))?
    }
}
