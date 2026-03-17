use anyhow::Result;
use aws_config::Region;
use aws_sdk_s3::Client;
use tracing::debug;

/// R2 configuration — loaded from env or ironmind.toml.
#[derive(Debug, Clone)]
pub struct R2Config {
    pub account_id: String,
    pub access_key_id: String,
    pub secret_access_key: String,
    pub bucket: String,
}

impl R2Config {
    /// Load from environment variables.
    /// IRONMIND_R2_ACCOUNT_ID, IRONMIND_R2_ACCESS_KEY_ID,
    /// IRONMIND_R2_SECRET_ACCESS_KEY, IRONMIND_R2_BUCKET
    pub fn from_env() -> Result<Self> {
        Ok(Self {
            account_id: std::env::var("IRONMIND_R2_ACCOUNT_ID")
                .map_err(|_| anyhow::anyhow!("IRONMIND_R2_ACCOUNT_ID not set"))?,
            access_key_id: std::env::var("IRONMIND_R2_ACCESS_KEY_ID")
                .map_err(|_| anyhow::anyhow!("IRONMIND_R2_ACCESS_KEY_ID not set"))?,
            secret_access_key: std::env::var("IRONMIND_R2_SECRET_ACCESS_KEY")
                .map_err(|_| anyhow::anyhow!("IRONMIND_R2_SECRET_ACCESS_KEY not set"))?,
            bucket: std::env::var("IRONMIND_R2_BUCKET").unwrap_or_else(|_| "ironmind".to_string()),
        })
    }
}

/// Low-level R2 client wrapping aws-sdk-s3.
/// R2 is S3-compatible: endpoint = https://<account_id>.r2.cloudflarestorage.com
#[derive(Clone)]
pub struct R2Client {
    pub(crate) inner: Client,
    pub(crate) bucket: String,
}

impl R2Client {
    pub async fn new(cfg: R2Config) -> Result<Self> {
        let endpoint = format!("https://{}.r2.cloudflarestorage.com", cfg.account_id);

        let creds = aws_credential_types::Credentials::new(
            &cfg.access_key_id,
            &cfg.secret_access_key,
            None,
            None,
            "ironmind",
        );

        let config = aws_config::defaults(aws_config::BehaviorVersion::latest())
            .region(Region::new("auto"))
            .endpoint_url(&endpoint)
            .credentials_provider(creds)
            .load()
            .await;

        let inner = Client::new(&config);
        Ok(Self {
            inner,
            bucket: cfg.bucket,
        })
    }

    /// Get an object as bytes. Returns None if not found.
    pub async fn get(&self, key: &str) -> Result<Option<Vec<u8>>> {
        debug!(key, "R2 get");
        match self
            .inner
            .get_object()
            .bucket(&self.bucket)
            .key(key)
            .send()
            .await
        {
            Ok(resp) => {
                let bytes = resp.body.collect().await?.into_bytes().to_vec();
                Ok(Some(bytes))
            }
            Err(e) => {
                let svc = e.as_service_error();
                if svc.map(|e| e.is_no_such_key()).unwrap_or(false) {
                    Ok(None)
                } else {
                    Err(anyhow::anyhow!("R2 get error for key {}: {}", key, e))
                }
            }
        }
    }

    /// Get an object as a UTF-8 string.
    pub async fn get_str(&self, key: &str) -> Result<Option<String>> {
        match self.get(key).await? {
            Some(bytes) => Ok(Some(String::from_utf8(bytes)?)),
            None => Ok(None),
        }
    }

    /// Put bytes at key.
    pub async fn put(&self, key: &str, data: Vec<u8>, content_type: &str) -> Result<()> {
        debug!(key, bytes = data.len(), "R2 put");
        self.inner
            .put_object()
            .bucket(&self.bucket)
            .key(key)
            .body(data.into())
            .content_type(content_type)
            .send()
            .await
            .map_err(|e| anyhow::anyhow!("R2 put error for key {}: {}", key, e))?;
        Ok(())
    }

    /// Put a JSON-serializable value.
    pub async fn put_json<T: serde::Serialize>(&self, key: &str, val: &T) -> Result<()> {
        let data = serde_json::to_vec(val)?;
        self.put(key, data, "application/json").await
    }

    /// Delete a key. No-ops if not found.
    pub async fn delete(&self, key: &str) -> Result<()> {
        debug!(key, "R2 delete");
        self.inner
            .delete_object()
            .bucket(&self.bucket)
            .key(key)
            .send()
            .await
            .map_err(|e| anyhow::anyhow!("R2 delete error for key {}: {}", key, e))?;
        Ok(())
    }

    /// List keys with a prefix. Returns up to 1000 keys.
    pub async fn list_keys(&self, prefix: &str) -> Result<Vec<String>> {
        debug!(prefix, "R2 list");
        let resp = self
            .inner
            .list_objects_v2()
            .bucket(&self.bucket)
            .prefix(prefix)
            .send()
            .await
            .map_err(|e| anyhow::anyhow!("R2 list error for prefix {}: {}", prefix, e))?;

        let keys = resp
            .contents()
            .iter()
            .filter_map(|obj| obj.key().map(|k| k.to_string()))
            .collect();
        Ok(keys)
    }
}
