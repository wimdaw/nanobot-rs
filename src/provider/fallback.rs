use super::{ChatRequest, ChatResponse, LlmProvider};
use anyhow::Result;
use async_trait::async_trait;
use std::sync::Arc;
use tracing::{warn, info};

pub struct FallbackProvider {
    primary: Arc<dyn LlmProvider>,
    fallbacks: Vec<(Arc<dyn LlmProvider>, String)>, // (provider, model_name)
}

impl FallbackProvider {
    pub fn new(primary: Arc<dyn LlmProvider>, fallbacks: Vec<(Arc<dyn LlmProvider>, String)>) -> Self {
        Self { primary, fallbacks }
    }
}

#[async_trait]
impl LlmProvider for FallbackProvider {
    fn name(&self) -> &str {
        self.primary.name()
    }

    async fn chat(&self, req: &ChatRequest) -> Result<ChatResponse> {
        match self.primary.chat(req).await {
            Ok(resp) => Ok(resp),
            Err(err) => {
                warn!(
                    "主模型 {} 调用失败: {}, 尝试故障转移降级链...",
                    req.model, err
                );

                for (fb_provider, fb_model) in &self.fallbacks {
                    info!("正在切换至备用模型: {} (提供商: {})", fb_model, fb_provider.name());
                    let mut fb_req = req.clone();
                    fb_req.model = fb_model.clone();

                    match fb_provider.chat(&fb_req).await {
                        Ok(fb_resp) => {
                            info!("✅ 备用模型 {} 响应成功！", fb_model);
                            return Ok(fb_resp);
                        }
                        Err(fb_err) => {
                            warn!("备用模型 {} 调用失败: {}", fb_model, fb_err);
                        }
                    }
                }

                Err(err)
            }
        }
    }
}
