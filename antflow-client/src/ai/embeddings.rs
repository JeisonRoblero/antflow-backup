use candle_core::{Device, Tensor};
use candle_nn::VarBuilder;
use candle_transformers::models::bert::{BertModel, Config};
use tokenizers::Tokenizer;
use anyhow::Result;

// Embed the model files into the binary (Phase 2 cached them)
const MODEL_BYTES: &[u8] = include_bytes!("../model/model.safetensors");
const CONFIG_BYTES: &[u8] = include_bytes!("../model/config.json");
const TOKENIZER_BYTES: &[u8] = include_bytes!("../model/tokenizer.json");

pub struct LocalEmbedder {
    model: BertModel,
    tokenizer: Tokenizer,
    device: Device,
}

impl LocalEmbedder {
    pub fn new() -> Result<Self> {
        let device = Device::Cpu;
        let config: Config = serde_json::from_slice(CONFIG_BYTES)?;
        let tokenizer = Tokenizer::from_bytes(TOKENIZER_BYTES).map_err(|e| anyhow::anyhow!(e))?;
        
        let vb = VarBuilder::from_slice_safetensors(MODEL_BYTES, candle_core::DType::F32, &device)?;
        let model = BertModel::load(vb, &config)?;
        
        Ok(Self {
            model,
            tokenizer,
            device,
        })
    }

    pub fn encode(&self, text: &str) -> Result<Vec<f32>> {
        let tokens = self.tokenizer.encode(text, true).map_err(|e| anyhow::anyhow!(e))?;
        let token_ids = tokens.get_ids();
        let attention_mask = tokens.get_attention_mask();
        
        let token_ids_tensor = Tensor::new(token_ids, &self.device)?.unsqueeze(0)?;
        let attention_mask_tensor = Tensor::new(attention_mask, &self.device)?.unsqueeze(0)?;
        let token_type_ids = Tensor::zeros(token_ids_tensor.shape(), candle_core::DType::U32, &self.device)?;
        
        // Inference pass
        let token_embeddings = self.model.forward(&token_ids_tensor, &token_type_ids, None)?;
        
        // Mean Pooling using attention mask
        let attention_mask_expanded = attention_mask_tensor
            .unsqueeze(2)?
            .to_dtype(candle_core::DType::F32)?;
            
        let sum_embeddings = token_embeddings.broadcast_mul(&attention_mask_expanded)?.sum(1)?;
        let sum_mask = attention_mask_expanded.sum(1)?;
        let sum_mask = sum_mask.clamp(1e-9f32, 1e9f32)?;
        
        let pooled = sum_embeddings.broadcast_div(&sum_mask)?;
        
        // L2 Normalization
        let norm = pooled.sqr()?.sum_keepdim(1)?.sqrt()?;
        let normalized = pooled.broadcast_div(&norm)?;
        
        // Extract to flat Vec<f32> (dimension size 384 for all-MiniLM-L6-v2)
        let vec = normalized.squeeze(0)?.to_vec1::<f32>()?;
        Ok(vec)
    }
}
