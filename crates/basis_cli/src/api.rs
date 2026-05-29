use anyhow::Result;
use serde::{Deserialize, Serialize};
use basis_store;

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct CreateNoteRequest {
    pub issuer_pubkey: String,
    pub recipient_pubkey: String,
    pub amount: u64,
    pub timestamp: u64,
    pub signature: String,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct SerializableIouNote {
    pub issuer_pubkey: String,
    pub recipient_pubkey: String,
    pub amount_collected: u64,
    pub amount_redeemed: u64,
    pub timestamp: u64,
    pub signature: String,
}

impl SerializableIouNote {
    pub fn outstanding_debt(&self) -> u64 {
        self.amount_collected.saturating_sub(self.amount_redeemed)
    }

    pub fn is_fully_redeemed(&self) -> bool {
        self.amount_collected == self.amount_redeemed
    }
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct KeyStatusResponse {
    pub total_debt: u64,
    pub collateral: u64,
    pub collateralization_ratio: f64,
    pub note_count: usize,
    pub last_updated: u64,
    pub issuer_pubkey: String,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct RedeemRequest {
    pub issuer_pubkey: String,
    pub recipient_pubkey: String,
    pub amount: u64,
    pub timestamp: u64,
    /// Reserve box ID (optional - will be looked up if not provided)
    #[serde(default)]
    pub reserve_box_id: String,
    /// Tracker box ID (optional - fetched by server)
    #[serde(default)]
    pub tracker_box_id: String,
    /// Tracker NFT ID from reserve box R6 register (optional - fetched by server)
    #[serde(default)]
    pub tracker_nft_id: String,
    /// Current blockchain height (optional - fetched by server)
    #[serde(default)]
    pub current_height: u64,
    /// Recipient address for redemption output (optional - derived from recipient_pubkey if not provided)
    #[serde(default)]
    pub recipient_address: String,
    /// Change address for transaction outputs (optional - server will derive from tracker pubkey if not provided)
    #[serde(default)]
    pub change_address: String,
    /// Issuer's Schnorr signature (65 bytes, hex encoded = 130 chars)
    pub issuer_signature: String,
    /// Whether this is an emergency redemption
    #[serde(default)]
    pub emergency: bool,
    /// Tracker's Schnorr signature (optional - server will generate if not provided)
    #[serde(default)]
    pub tracker_signature: Option<String>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct RedeemResponse {
    pub redemption_id: String,
    pub amount: u64,
    pub timestamp: u64,
    pub proof_available: bool,
    pub transaction_pending: bool,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct CompleteRedemptionRequest {
    pub issuer_pubkey: String,
    pub recipient_pubkey: String,
    pub redeemed_amount: u64,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct CreateReserveRequest {
    pub nft_id: String,
    pub owner_pubkey: String,
    pub erg_amount: u64,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ReserveCreationResponse {
    pub requests: Vec<ReservePaymentRequest>,
    pub fee: u64,
    pub change_address: String,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ReservePaymentRequest {
    pub address: String,
    pub value: u64,
    pub assets: Vec<Asset>,
    pub registers: std::collections::HashMap<String, String>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Asset {
    pub token_id: String,
    pub amount: u64,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ProofResponse {
    pub issuer_pubkey: String,
    pub recipient_pubkey: String,
    pub proof_data: String,
    pub tracker_state_digest: String,
    pub block_height: u64,
    pub timestamp: u64,
}

// Tracker signature request/response for redemption
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct TrackerSignatureRequest {
    pub issuer_pubkey: String,
    pub recipient_pubkey: String,
    pub total_debt: u64,
    /// Payment timestamp in milliseconds since Unix epoch
    pub timestamp: u64,
    #[serde(default)]
    pub emergency: bool,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct TrackerSignatureResponse {
    pub success: bool,
    pub tracker_signature: String,
    pub tracker_pubkey: String,
    pub message_signed: String,
    pub is_emergency: Option<bool>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct TrackerEvent {
    pub id: u64,
    pub event_type: String,
    pub timestamp: u64,
    pub issuer_pubkey: Option<String>,
    pub recipient_pubkey: Option<String>,
    pub amount: Option<u64>,
    pub reserve_box_id: Option<String>,
    pub collateral_amount: Option<u64>,
    pub redeemed_amount: Option<u64>,
    pub height: Option<u64>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ApiResponse<T> {
    pub success: bool,
    pub data: Option<T>,
    pub error: Option<String>,
}

#[derive(Debug)]
pub struct TrackerClient {
    base_url: String,
}

impl TrackerClient {
    pub fn new(base_url: String) -> Self {
        Self { base_url }
    }

    pub async fn health_check(&self) -> Result<bool> {
        let url = format!("{}/", self.base_url);
        let response = ureq::get(&url).call()?;

        Ok(response.status() == 200)
    }

    // Note operations
    pub async fn create_note(&self, request: CreateNoteRequest) -> Result<()> {
        let url = format!("{}/notes", self.base_url);
        let response = ureq::post(&url).send_json(serde_json::to_value(request)?)?;

        if response.status() == 200 || response.status() == 201 {
            Ok(())
        } else {
            let error_text = response.into_string()?;
            Err(anyhow::anyhow!("Failed to create note: {}", error_text))
        }
    }

    pub async fn get_issuer_notes(&self, pubkey: &str) -> Result<Vec<SerializableIouNote>> {
        let url = format!("{}/notes/issuer/{}", self.base_url, pubkey);
        let response = ureq::get(&url).call()?;

        if response.status() == 200 {
            let api_response: ApiResponse<Vec<SerializableIouNote>> = response.into_json()?;
            if api_response.success {
                Ok(api_response.data.unwrap_or_default())
            } else {
                Err(anyhow::anyhow!("API error: {:?}", api_response.error))
            }
        } else {
            let error_text = response.into_string()?;
            Err(anyhow::anyhow!(
                "Failed to get issuer notes: {}",
                error_text
            ))
        }
    }

    pub async fn get_recipient_notes(&self, pubkey: &str) -> Result<Vec<SerializableIouNote>> {
        let url = format!("{}/notes/recipient/{}", self.base_url, pubkey);
        let response = ureq::get(&url).call()?;

        if response.status() == 200 {
            let api_response: ApiResponse<Vec<SerializableIouNote>> = response.into_json()?;
            if api_response.success {
                Ok(api_response.data.unwrap_or_default())
            } else {
                Err(anyhow::anyhow!("API error: {:?}", api_response.error))
            }
        } else {
            let error_text = response.into_string()?;
            Err(anyhow::anyhow!(
                "Failed to get recipient notes: {}",
                error_text
            ))
        }
    }

    pub async fn get_note(
        &self,
        issuer: &str,
        recipient: &str,
    ) -> Result<Option<SerializableIouNote>> {
        let url = format!(
            "{}/notes/issuer/{}/recipient/{}",
            self.base_url, issuer, recipient
        );
        let response = ureq::get(&url).call()?;

        if response.status() == 200 {
            let api_response: ApiResponse<Option<SerializableIouNote>> = response.into_json()?;
            if api_response.success {
                Ok(api_response.data.unwrap_or(None))
            } else {
                Err(anyhow::anyhow!("API error: {:?}", api_response.error))
            }
        } else {
            let error_text = response.into_string()?;
            Err(anyhow::anyhow!("Failed to get note: {}", error_text))
        }
    }

    // Reserve operations
    pub async fn get_reserve_status(&self, pubkey: &str) -> Result<KeyStatusResponse> {
        let url = format!("{}/key-status/{}", self.base_url, pubkey);
        let response = ureq::get(&url).call()?;

        if response.status() == 200 {
            let api_response: ApiResponse<KeyStatusResponse> = response.into_json()?;
            if api_response.success {
                Ok(api_response.data.unwrap())
            } else {
                Err(anyhow::anyhow!("API error: {:?}", api_response.error))
            }
        } else {
            let error_text = response.into_string()?;
            Err(anyhow::anyhow!(
                "Failed to get reserve status: {}",
                error_text
            ))
        }
    }

    // Redemption
    pub async fn initiate_redemption(&self, request: RedeemRequest) -> Result<RedeemResponse> {
        let url = format!("{}/redeem", self.base_url);
        let response = match ureq::post(&url).send_json(serde_json::to_value(request)?) {
            Ok(resp) => resp,
            Err(ureq::Error::Status(code, resp)) => {
                let error_text = resp.into_string().unwrap_or_else(|_| format!("HTTP {}", code));
                return Err(anyhow::anyhow!(
                    "Failed to initiate redemption: {}",
                    error_text
                ));
            }
            Err(e) => {
                return Err(anyhow::anyhow!("Request failed: {}", e));
            }
        };

        if response.status() == 200 {
            let api_response: ApiResponse<RedeemResponse> = response.into_json()?;
            if api_response.success {
                Ok(api_response.data.unwrap())
            } else {
                Err(anyhow::anyhow!("API error: {:?}", api_response.error))
            }
        } else {
            let error_text = response.into_string()?;
            Err(anyhow::anyhow!(
                "Failed to initiate redemption: {}",
                error_text
            ))
        }
    }

    pub async fn complete_redemption(&self, request: CompleteRedemptionRequest) -> Result<()> {
        let url = format!("{}/redeem/complete", self.base_url);
        let response = match ureq::post(&url).send_json(serde_json::to_value(request)?) {
            Ok(resp) => resp,
            Err(ureq::Error::Status(code, resp)) => {
                let error_text = resp.into_string().unwrap_or_else(|_| format!("HTTP {}", code));
                return Err(anyhow::anyhow!(
                    "Failed to complete redemption: {}",
                    error_text
                ));
            }
            Err(e) => {
                return Err(anyhow::anyhow!("Request failed: {}", e));
            }
        };

        if response.status() == 200 {
            Ok(())
        } else {
            let error_text = response.into_string()?;
            Err(anyhow::anyhow!(
                "Failed to complete redemption: {}",
                error_text
            ))
        }
    }

    /// Request tracker signature for redemption
    /// Following the Basis protocol specification - POST /tracker/signature
    pub async fn request_tracker_signature(
        &self,
        issuer_pubkey: &str,
        recipient_pubkey: &str,
        total_debt: u64,
        timestamp: u64,
        emergency: bool,
    ) -> Result<TrackerSignatureResponse> {
        let request = TrackerSignatureRequest {
            issuer_pubkey: issuer_pubkey.to_string(),
            recipient_pubkey: recipient_pubkey.to_string(),
            total_debt,
            timestamp,
            emergency,
        };

        let url = format!("{}/tracker/signature", self.base_url);
        let response = ureq::post(&url).send_json(serde_json::to_value(request)?)?;

        if response.status() == 200 {
            let api_response: ApiResponse<TrackerSignatureResponse> = response.into_json()?;
            if api_response.success {
                Ok(api_response.data.unwrap())
            } else {
                Err(anyhow::anyhow!("API error: {:?}", api_response.error))
            }
        } else {
            let error_text = response.into_string()?;
            Err(anyhow::anyhow!(
                "Failed to request tracker signature: {}",
                error_text
            ))
        }
    }
}

// Define structs outside of the impl block
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct RedemptionPreparationRequest {
    pub issuer_pubkey: String,
    pub recipient_pubkey: String,
    pub amount: u64,
    pub timestamp: u64,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct RedemptionPreparationResponse {
    pub redemption_id: String,
    pub avl_proof: String,  // Hex-encoded AVL proof
    pub tracker_signature: String,  // Hex-encoded 65-byte Schnorr signature
    pub tracker_pubkey: String,  // Hex-encoded tracker public key
    pub tracker_state_digest: String,  // Hex-encoded 33-byte AVL tree root digest
    pub block_height: u64,
    pub tracker_box_id: String,  // ID of the tracker box used for the proof
}

// Tracker proof response for context var #8
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct TrackerProofResponse {
    pub key: String,
    pub value: String,
    pub proof: String,
    pub total_debt: u64,
    pub tracker_state_digest: String,
}

// Reserve proof response for context var #5 (insert) and #7 (lookup)
// GET /reserve/proof endpoint response
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ReserveProofResponse {
    /// Hex-encoded AVL tree key: hash(ownerKey || receiverKey)
    pub key: String,
    /// Hex-encoded value: already_redeemed as 8-byte big-endian
    pub value: String,
    /// Hex-encoded AVL proof bytes (None for first redemption) - for context var #7 (lookup)
    pub proof: Option<String>,
    /// Already redeemed amount as integer
    pub already_redeemed: u64,
    /// Whether this is the first redemption (no lookup proof needed)
    pub is_first_redemption: bool,
    /// Hex-encoded AVL insert proof for context var #5 (insert operation)
    /// This proof is used to INSERT the new already_redeemed amount into the reserve tree
    pub insert_proof: String,
}

impl TrackerClient {
    /// Get tracker lookup proof for context var #8
    pub async fn get_tracker_proof(&self, issuer_pubkey: &str, recipient_pubkey: &str) -> Result<TrackerProofResponse> {
        let url = format!(
            "{}/tracker/proof?issuer_pubkey={}&recipient_pubkey={}",
            self.base_url, issuer_pubkey, recipient_pubkey
        );
        let response = ureq::get(&url).call()?;

        if response.status() == 200 {
            let api_response: ApiResponse<TrackerProofResponse> = response.into_json()?;
            if api_response.success {
                Ok(api_response.data.unwrap())
            } else {
                Err(anyhow::anyhow!("API error: {:?}", api_response.error))
            }
        } else {
            let error_text = response.into_string()?;
            Err(anyhow::anyhow!("Failed to get tracker proof: {}", error_text))
        }
    }

    /// Get reserve proof for context var #5 (insert) and #7 (lookup)
    pub async fn get_reserve_proof(&self, issuer_pubkey: &str, recipient_pubkey: &str) -> Result<ReserveProofResponse> {
        let url = format!(
            "{}/reserve/proof?issuer_pubkey={}&recipient_pubkey={}",
            self.base_url, issuer_pubkey, recipient_pubkey
        );
        let response = ureq::get(&url).call()?;

        if response.status() == 200 {
            let api_response: ApiResponse<ReserveProofResponse> = response.into_json()?;
            if api_response.success {
                Ok(api_response.data.unwrap())
            } else {
                // If reserve not found, this might be first redemption
                Err(anyhow::anyhow!("Reserve record not found: {:?}", api_response.error))
            }
        } else {
            let error_text = response.into_string()?;
            Err(anyhow::anyhow!("Failed to get reserve proof: {}", error_text))
        }
    }

    pub async fn prepare_redemption(&self, issuer_pubkey: &str, recipient_pubkey: &str, amount: u64) -> Result<RedemptionPreparationResponse> {
        let request = RedemptionPreparationRequest {
            issuer_pubkey: issuer_pubkey.to_string(),
            recipient_pubkey: recipient_pubkey.to_string(),
            amount,
            timestamp: std::time::SystemTime::now()
                .duration_since(std::time::UNIX_EPOCH)
                .unwrap()
                .as_secs(),
        };

        let url = format!("{}/redemption/prepare", self.base_url);
        let response = ureq::post(&url).send_json(serde_json::to_value(request)?)?;

        if response.status() == 200 {
            let api_response: ApiResponse<RedemptionPreparationResponse> = response.into_json()?;
            if api_response.success {
                Ok(api_response.data.unwrap())
            } else {
                Err(anyhow::anyhow!("API error during redemption preparation: {:?}", api_response.error))
            }
        } else {
            let error_text = response.into_string()?;
            Err(anyhow::anyhow!(
                "Failed to prepare redemption: {}",
                error_text
            ))
        }
    }

    // Events & Status
    pub async fn get_events(&self, page: usize, page_size: usize) -> Result<Vec<TrackerEvent>> {
        let url = format!(
            "{}/events/paginated?page={}&page_size={}",
            self.base_url, page, page_size
        );
        let response = ureq::get(&url).call()?;

        if response.status() == 200 {
            let api_response: ApiResponse<Vec<TrackerEvent>> = response.into_json()?;
            if api_response.success {
                Ok(api_response.data.unwrap_or_default())
            } else {
                Err(anyhow::anyhow!("API error: {:?}", api_response.error))
            }
        } else {
            let error_text = response.into_string()?;
            Err(anyhow::anyhow!("Failed to get events: {}", error_text))
        }
    }

    pub async fn get_recent_events(&self) -> Result<Vec<TrackerEvent>> {
        let url = format!("{}/events", self.base_url);
        let response = ureq::get(&url).call()?;

        if response.status() == 200 {
            let api_response: ApiResponse<Vec<TrackerEvent>> = response.into_json()?;
            if api_response.success {
                Ok(api_response.data.unwrap_or_default())
            } else {
                Err(anyhow::anyhow!("API error: {:?}", api_response.error))
            }
        } else {
            let error_text = response.into_string()?;
            Err(anyhow::anyhow!(
                "Failed to get recent events: {}",
                error_text
            ))
        }
    }

    // Reserve operations
    pub async fn create_reserve(&self, request: CreateReserveRequest) -> Result<ReserveCreationResponse> {
        let url = format!("{}/reserves/create", self.base_url);
        let response = ureq::post(&url).send_json(serde_json::to_value(request)?)?;

        if response.status() == 200 {
            let api_response: ApiResponse<ReserveCreationResponse> = response.into_json()?;
            if api_response.success {
                Ok(api_response.data.unwrap())
            } else {
                Err(anyhow::anyhow!("API error: {:?}", api_response.error))
            }
        } else {
            let error_text = response.into_string()?;
            Err(anyhow::anyhow!("Failed to create reserve: {}", error_text))
        }
    }
}

// Define the TrackerBoxIdResponse struct outside of the impl block
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct TrackerBoxIdResponse {
    pub tracker_box_id: String,
    pub timestamp: u64,
    pub height: u64,
}

// Define helper structs for API response handling
#[derive(Debug, Clone, Serialize, Deserialize)]
struct FlattenedReserveInfo {
    pub box_id: String,
    pub owner_pubkey: String,
    pub collateral_amount: u64,
    pub total_debt: u64,
    pub tracker_nft_id: Option<String>,
    pub last_updated_height: u64,
    pub last_updated_timestamp: u64,
    pub collateralization_ratio: Option<f64>,
}

impl From<FlattenedReserveInfo> for basis_store::ExtendedReserveInfo {
    fn from(flattened: FlattenedReserveInfo) -> Self {
        use basis_store::{ReserveInfo, ExtendedReserveInfo};

        let base_info = ReserveInfo {
            collateral_amount: flattened.collateral_amount,
            last_updated_height: flattened.last_updated_height,
            contract_address: String::new(), // Set by get_reserves_by_issuer() after fetching from server config
            tracker_nft_id: flattened.tracker_nft_id.unwrap_or_default(),
        };

        ExtendedReserveInfo {
            base_info,
            total_debt: flattened.total_debt,
            box_id: flattened.box_id,
            owner_pubkey: flattened.owner_pubkey,
            last_updated_timestamp: flattened.last_updated_timestamp,
        }
    }
}

impl TrackerClient {
    // New methods for the redemption transaction generation

    /// Get reserves for a specific issuer
    pub async fn get_reserves_by_issuer(&self, pubkey: &str) -> Result<Vec<basis_store::ExtendedReserveInfo>> {
        let url = format!("{}/reserves/issuer/{}", self.base_url, pubkey);
        let response = ureq::get(&url).call()?;

        if response.status() == 200 {
            let api_response: ApiResponse<Vec<FlattenedReserveInfo>> = response.into_json()?;
            if api_response.success {
                let flattened_reserves = api_response.data.unwrap_or_default();
                
                // Fetch reserve contract P2S address from server config
                let contract_address = match self.get_basis_reserve_contract_p2s().await {
                    Ok(addr) => addr,
                    Err(e) => {
                        eprintln!("⚠️  Failed to get reserve contract P2S address: {}", e);
                        String::new()
                    }
                };
                
                let extended_reserves: Vec<basis_store::ExtendedReserveInfo> = flattened_reserves
                    .into_iter()
                    .map(|flattened| {
                        let mut reserve = basis_store::ExtendedReserveInfo::from(flattened);
                        reserve.base_info.contract_address = contract_address.clone();
                        reserve
                    })
                    .collect();
                Ok(extended_reserves)
            } else {
                Err(anyhow::anyhow!("API error: {:?}", api_response.error))
            }
        } else {
            let error_text = response.into_string()?;
            Err(anyhow::anyhow!(
                "Failed to get reserves for issuer {}: {}",
                pubkey, error_text
            ))
        }
    }

    pub async fn get_latest_tracker_box_id(&self) -> Result<TrackerBoxIdResponse> {
        let url = format!("{}/tracker/latest-box-id", self.base_url);
        let response = ureq::get(&url).call()?;

        if response.status() == 200 {
            let api_response: ApiResponse<TrackerBoxIdResponse> = response.into_json()?;
            if api_response.success {
                Ok(api_response.data.unwrap())
            } else {
                Err(anyhow::anyhow!("API error: {:?}", api_response.error))
            }
        } else if response.status() == 404 {
            let error_text = response.into_string()?;
            Err(anyhow::anyhow!("No tracker box found: {}", error_text))
        } else {
            let error_text = response.into_string()?;
            Err(anyhow::anyhow!(
                "Failed to get latest tracker box ID: {}",
                error_text
            ))
        }
    }

    /// Get the Basis reserve contract P2S address from the server configuration
    pub async fn get_basis_reserve_contract_p2s(&self) -> Result<String> {
        let url = format!("{}/config/reserve-contract-p2s", self.base_url);
        let response = ureq::get(&url).call()?;

        if response.status() == 200 {
            let api_response: ApiResponse<String> = response.into_json()?;
            if api_response.success {
                Ok(api_response.data.unwrap())
            } else {
                Err(anyhow::anyhow!("API error: {:?}", api_response.error))
            }
        } else {
            let error_text = response.into_string()?;
            Err(anyhow::anyhow!(
                "Failed to get reserve contract P2S address: {}",
                error_text
            ))
        }
    }

    /// Get box details from the Ergo node directly
    pub async fn get_box_from_node(&self, box_id: &str, node_url: &str, api_key: Option<&str>) -> Result<ErgoBoxDetails> {
        let url = format!("{}/utxo/byId/{}", node_url.trim_end_matches('/'), box_id);
        let mut request_builder = ureq::get(&url);

        // Add API key if provided
        if let Some(key) = api_key {
            request_builder = request_builder.set("api_key", key);
        }

        let response = request_builder.call()?;

        if response.status() == 200 {
            let box_details: ErgoBoxDetails = response.into_json()?;
            Ok(box_details)
        } else {
            let error_text = response.into_string()?;
            Err(anyhow::anyhow!(
                "Failed to get box from node {}: {}",
                box_id, error_text
            ))
        }
    }

    /// Get the serialized bytes of a box from the Ergo node
    /// Makes direct request to Ergo node's /utxo/byId/{box_id} endpoint
    pub async fn get_box_bytes(&self, box_id: &str, node_url: &str, api_key: Option<&str>) -> Result<String> {
        let url = format!("{}/utxo/byId/{}", node_url, box_id);
        
        let mut request = ureq::get(&url);
        
        // Add API key if provided
        if let Some(key) = api_key {
            request = request.set("api_key", key);
        }
        
        let response = request.call()?;
        
        if response.status() == 200 {
            // Return the box JSON as string (the Ergo node /wallet/transaction/sign 
            // accepts box IDs in inputsRaw/dataInputsRaw, not full serialized bytes)
            let box_json = response.into_string()?;
            Ok(box_json)
        } else {
            Err(anyhow::anyhow!("Failed to get box {} from Ergo node: status {}", box_id, response.status()))
        }
    }

    pub async fn get_all_notes(&self) -> Result<Vec<SerializableIouNoteWithAge>> {
        let url = format!("{}/notes", self.base_url);
        let response = ureq::get(&url).call()?;

        if response.status() == 200 {
            let api_response: ApiResponse<Vec<SerializableIouNoteWithAge>> = response.into_json()?;
            if api_response.success {
                Ok(api_response.data.unwrap_or_default())
            } else {
                Err(anyhow::anyhow!("API error: {:?}", api_response.error))
            }
        } else {
            let error_text = response.into_string()?;
            Err(anyhow::anyhow!(
                "Failed to get all notes: {}",
                error_text
            ))
        }
    }
}

// Define the ErgoBoxDetails struct for parsing box data from the Ergo node
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ErgoBoxDetails {
    pub box_id: String,
    pub value: u64,
    pub ergo_tree: String,
    pub assets: Vec<Token>,
    pub additional_registers: std::collections::HashMap<String, String>,
    pub creation_height: u32,
    pub transaction_id: String,
    pub index: u16,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Token {
    pub token_id: String,
    pub amount: u64,
}

// Define the SerializableIouNoteWithAge struct outside of the impl block
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct SerializableIouNoteWithAge {
    pub issuer_pubkey: String,  // Changed from issuer_pubkey to match server response
    pub recipient_pubkey: String,  // Changed from recipient_pubkey to match server response
    pub amount_collected: u64,
    pub amount_redeemed: u64,
    pub timestamp: u64,
    pub signature: String,
    pub age_seconds: u64,
}

impl SerializableIouNoteWithAge {
    /// Calculate the outstanding debt (amount collected minus amount redeemed)
    pub fn outstanding_debt(&self) -> u64 {
        self.amount_collected.saturating_sub(self.amount_redeemed)
    }
}
