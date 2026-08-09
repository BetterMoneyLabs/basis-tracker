use anyhow::Result;
use basis_store;
use serde::{Deserialize, Serialize};

const V1_REDEMPTION_RETIRED: &str =
    "Basis v1 redemption is retired; a fully validated v2 manifest and confirmed-chain authority are required before proof generation or signing";

fn reject_retired_v1_redemption<T>() -> Result<T> {
    Err(anyhow::anyhow!(V1_REDEMPTION_RETIRED))
}

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

    #[allow(dead_code)]
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
    #[serde(default)]
    pub has_pending_refund: bool,
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

/// Legacy response shape retained by the unconditional v1 tombstone.
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

// Upload policy request/response for acceptance predicates
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct UploadPolicyRequest {
    pub recipient_pubkey: String,
    pub policy_json: String,
    pub signature: String,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct UploadPolicyResponse {
    pub uploaded_at: u64,
    pub policy_hash: String,
}

// Response for getting a recipient's acceptance policy
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct GetPolicyResponse {
    pub recipient_pubkey: String,
    pub policy_json: String,
    pub signature: String,
    pub uploaded_at: u64,
}

// Request for checking note acceptance
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct CheckAcceptanceRequest {
    pub issuer_pubkey: String,
    pub total_debt: u64,
    #[serde(default)]
    pub recipient_pubkey: Option<String>,
}

// Response for checking note acceptance
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct CheckAcceptanceResponse {
    pub acceptable: bool,
    pub reason: Option<String>,
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

    /// Retired v1 tracker-signature call.
    pub async fn request_tracker_signature(
        &self,
        _issuer_pubkey: &str,
        _recipient_pubkey: &str,
        _total_debt: u64,
        _timestamp: u64,
        _emergency: bool,
    ) -> Result<TrackerSignatureResponse> {
        reject_retired_v1_redemption()
    }
}

/// Legacy response shape retained by the unconditional v1 tombstone.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct RedemptionPreparationResponse {
    pub redemption_id: String,
    pub avl_proof: String,            // Hex-encoded AVL proof
    pub tracker_signature: String,    // Hex-encoded 65-byte Schnorr signature
    pub tracker_pubkey: String,       // Hex-encoded tracker public key
    pub tracker_state_digest: String, // Hex-encoded 33-byte AVL tree root digest
    pub block_height: u64,
    pub tracker_box_id: String, // ID of the tracker box used for the proof
}

/// Legacy request shape retained by the unconditional v1 tombstone.
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct RedemptionBuildRequest {
    pub issuer_pubkey: String,
    pub recipient_pubkey: String,
    pub amount: u64,
    pub timestamp: u64,
    /// Issuer's 65-byte Schnorr signature (hex) over the redemption signing message.
    pub issuer_signature: String,
    #[serde(default)]
    pub emergency: bool,
    #[serde(default)]
    pub tracker_box_id: Option<String>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct RedemptionBuildResponse {
    pub unsigned_tx: serde_json::Value,
    pub partial_tx: serde_json::Value,
    pub input_box_binaries: Vec<String>,
    pub data_box_binaries: Vec<String>,
    pub headers: Vec<ergo_lib::ergo_chain_types::Header>,
    pub reserve_box_id: String,
    pub tracker_box_id: String,
    pub reserve_output_value: u64,
    pub recipient_output_value: u64,
    pub total_debt: u64,
    pub change_amount: u64,
    pub change_address: String,
    pub recipient_address: String,
    pub is_first_redemption: bool,
    pub fee: u64,
    /// Legacy response field; no successful v1 build response is produced.
    #[serde(default)]
    pub new_already_redeemed: u64,
}

/// Legacy response shape retained by the unconditional v1 tombstone.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct TrackerProofResponse {
    pub key: String,
    pub value: String,
    pub proof: String,
    pub total_debt: u64,
    pub tracker_state_digest: String,
}

/// Legacy response shape retained by the unconditional v1 tombstone.
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
    /// Hex-encoded updated reserve state digest after the insert operation (R5 register value)
    pub new_reserve_state_digest: String,
}

impl TrackerClient {
    /// Retired v1 tracker-proof call.
    pub async fn get_tracker_proof(
        &self,
        _issuer_pubkey: &str,
        _recipient_pubkey: &str,
    ) -> Result<TrackerProofResponse> {
        reject_retired_v1_redemption()
    }

    /// Retired v1 reserve-proof call.
    pub async fn get_reserve_proof(
        &self,
        _issuer_pubkey: &str,
        _recipient_pubkey: &str,
        _amount: u64,
        _timestamp: u64,
    ) -> Result<ReserveProofResponse> {
        reject_retired_v1_redemption()
    }

    /// Retired v1 redemption-preparation call.
    pub async fn prepare_redemption(
        &self,
        _issuer_pubkey: &str,
        _recipient_pubkey: &str,
        _amount: u64,
    ) -> Result<RedemptionPreparationResponse> {
        reject_retired_v1_redemption()
    }

    /// Retired v1 transaction-build call.
    pub async fn redemption_build(
        &self,
        _request: RedemptionBuildRequest,
    ) -> Result<RedemptionBuildResponse> {
        reject_retired_v1_redemption()
    }

    /// Retired v1 transaction-submit call.
    pub async fn redemption_submit(&self, _signed_tx: serde_json::Value) -> Result<String> {
        reject_retired_v1_redemption()
    }

    // Events & Status
    #[allow(dead_code)]
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
    pub async fn create_reserve(
        &self,
        request: CreateReserveRequest,
    ) -> Result<ReserveCreationResponse> {
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

    /// Upload acceptance policy to server
    #[allow(dead_code)]
    pub async fn upload_policy(
        &self,
        request: UploadPolicyRequest,
    ) -> Result<UploadPolicyResponse> {
        let url = format!("{}/acceptance/policy", self.base_url);
        let response = ureq::post(&url).send_json(serde_json::to_value(request)?)?;

        if response.status() == 200 {
            let api_response: ApiResponse<UploadPolicyResponse> = response.into_json()?;
            if api_response.success {
                Ok(api_response.data.unwrap())
            } else {
                Err(anyhow::anyhow!("API error: {:?}", api_response.error))
            }
        } else {
            let error_text = response.into_string()?;
            Err(anyhow::anyhow!("Failed to upload policy: {}", error_text))
        }
    }

    /// Get acceptance policy for a recipient from the server
    #[allow(dead_code)]
    pub async fn get_policy(&self, recipient_pubkey: &str) -> Result<GetPolicyResponse> {
        let url = format!("{}/acceptance/policy/{}", self.base_url, recipient_pubkey);
        let response = ureq::get(&url).call()?;

        if response.status() == 200 {
            let api_response: ApiResponse<GetPolicyResponse> = response.into_json()?;
            if api_response.success {
                Ok(api_response.data.unwrap())
            } else {
                Err(anyhow::anyhow!("API error: {:?}", api_response.error))
            }
        } else if response.status() == 404 {
            Err(anyhow::anyhow!("No policy found for this recipient"))
        } else {
            let error_text = response.into_string()?;
            Err(anyhow::anyhow!("Failed to get policy: {}", error_text))
        }
    }

    /// Check if a note would be accepted by the server's acceptance policy
    #[allow(dead_code)]
    pub async fn check_acceptance(
        &self,
        issuer_pubkey: &str,
        total_debt: u64,
        recipient_pubkey: Option<&str>,
    ) -> Result<CheckAcceptanceResponse> {
        let request = CheckAcceptanceRequest {
            issuer_pubkey: issuer_pubkey.to_string(),
            total_debt,
            recipient_pubkey: recipient_pubkey.map(|s| s.to_string()),
        };

        let url = format!("{}/acceptance/check", self.base_url);
        let response = ureq::post(&url).send_json(serde_json::to_value(request)?)?;

        if response.status() == 200 {
            let api_response: ApiResponse<CheckAcceptanceResponse> = response.into_json()?;
            if api_response.success {
                Ok(api_response.data.unwrap())
            } else {
                Err(anyhow::anyhow!("API error: {:?}", api_response.error))
            }
        } else {
            let error_text = response.into_string()?;
            Err(anyhow::anyhow!(
                "Failed to check acceptance: {}",
                error_text
            ))
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
    #[serde(alias = "boxId")]
    pub box_id: String,
    pub owner_pubkey: String,
    pub collateral_amount: u64,
    pub total_debt: u64,
    pub tracker_nft_id: Option<String>,
    pub last_updated_height: u64,
    pub last_updated_timestamp: u64,
    pub collateralization_ratio: Option<f64>,
    #[serde(default)]
    pub refund_initiation_height: u64,
}

fn decode_box_id(raw: &str) -> String {
    if raw.len() == 128 {
        if let Ok(bytes) = hex::decode(raw) {
            if bytes.iter().all(|b| b.is_ascii_hexdigit()) {
                return String::from_utf8(bytes).unwrap_or_else(|_| raw.to_string());
            }
        }
    }
    raw.to_string()
}

impl From<FlattenedReserveInfo> for basis_store::ExtendedReserveInfo {
    fn from(flattened: FlattenedReserveInfo) -> Self {
        use basis_store::{ExtendedReserveInfo, ReserveInfo};

        let base_info = ReserveInfo {
            collateral_amount: flattened.collateral_amount,
            last_updated_height: flattened.last_updated_height,
            contract_address: String::new(), // Set by get_reserves_by_issuer() after fetching from server config
            tracker_nft_id: flattened.tracker_nft_id.unwrap_or_default(),
            refund_initiation_height: flattened.refund_initiation_height,
        };

        ExtendedReserveInfo {
            base_info,
            total_debt: flattened.total_debt,
            box_id: decode_box_id(&flattened.box_id),
            owner_pubkey: flattened.owner_pubkey,
            last_updated_timestamp: flattened.last_updated_timestamp,
        }
    }
}

impl TrackerClient {
    // New methods for the redemption transaction generation

    /// Get reserves for a specific issuer
    pub async fn get_reserves_by_issuer(
        &self,
        pubkey: &str,
    ) -> Result<Vec<basis_store::ExtendedReserveInfo>> {
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
                        reserve.box_id = decode_box_id(&reserve.box_id);
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
                pubkey,
                error_text
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

    /// Get unspent boxes from the node's wallet.
    /// This follows `/wallet/boxes/unspent` which returns boxes the wallet can sign.
    /// Each item has a nested `box` object containing the actual ErgoBoxDetails.
    pub async fn get_wallet_boxes(
        &self,
        node_url: &str,
        api_key: Option<&str>,
    ) -> Result<Vec<ErgoBoxDetails>> {
        let url = format!(
            "{}/wallet/boxes/unspent?minConfirmations=0&maxConfirmations=-1",
            node_url.trim_end_matches('/')
        );

        let mut request = ureq::get(&url);
        if let Some(key) = api_key {
            request = request.set("api_key", key);
        }

        let response = request.call()?;

        if response.status() == 200 {
            #[derive(Debug, Clone, Serialize, Deserialize)]
            struct WalletBoxEntry {
                #[serde(rename = "box")]
                pub box_details: ErgoBoxDetails,
            }
            let entries: Vec<WalletBoxEntry> = response.into_json()?;
            Ok(entries.into_iter().map(|e| e.box_details).collect())
        } else {
            let error_text = response.into_string()?;
            Err(anyhow::anyhow!(
                "Failed to get wallet boxes: {}",
                error_text
            ))
        }
    }

    /// Get box details from the Ergo node directly
    pub async fn get_box_from_node(
        &self,
        box_id: &str,
        node_url: &str,
        api_key: Option<&str>,
    ) -> Result<ErgoBoxDetails> {
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
                box_id,
                error_text
            ))
        }
    }

    /// Get the hex-encoded serialized bytes of a box from the Ergo node
    /// using the /utxo/byIdBinary/{box_id} endpoint.
    pub async fn get_box_binary(
        &self,
        box_id: &str,
        node_url: &str,
        api_key: Option<&str>,
    ) -> Result<String> {
        let url = format!(
            "{}/utxo/byIdBinary/{}",
            node_url.trim_end_matches('/'),
            box_id
        );

        let mut request = ureq::get(&url);

        // Add API key if provided
        if let Some(key) = api_key {
            request = request.set("api_key", key);
        }

        let response = request.call()?;

        if response.status() == 200 {
            let body: serde_json::Value = response.into_json()?;
            body["bytes"]
                .as_str()
                .map(|s| s.to_string())
                .ok_or_else(|| {
                    anyhow::anyhow!(
                        "Missing 'bytes' field in /utxo/byIdBinary response for box {}",
                        box_id
                    )
                })
        } else {
            Err(anyhow::anyhow!(
                "Failed to get binary box {} from Ergo node: status {}",
                box_id,
                response.status()
            ))
        }
    }

    /// Get current blockchain height from the Ergo node /info endpoint.
    pub async fn get_node_height(&self, node_url: &str, api_key: Option<&str>) -> Result<u32> {
        let url = format!("{}/info", node_url.trim_end_matches('/'));

        let mut request = ureq::get(&url);

        if let Some(key) = api_key {
            request = request.set("api_key", key);
        }

        let response = request.call()?;

        if response.status() == 200 {
            let body: serde_json::Value = response.into_json()?;
            body["fullHeight"]
                .as_u64()
                .map(|h| h as u32)
                .ok_or_else(|| anyhow::anyhow!("Missing 'fullHeight' in node /info response"))
        } else {
            Err(anyhow::anyhow!(
                "Failed to get node height: status {}",
                response.status()
            ))
        }
    }

    pub async fn get_all_notes(&self) -> Result<Vec<SerializableIouNoteWithAge>> {
        let url = format!("{}/notes", self.base_url);
        let response = ureq::get(&url).call()?;

        if response.status() == 200 {
            let api_response: ApiResponse<Vec<SerializableIouNoteWithAge>> =
                response.into_json()?;
            if api_response.success {
                Ok(api_response.data.unwrap_or_default())
            } else {
                Err(anyhow::anyhow!("API error: {:?}", api_response.error))
            }
        } else {
            let error_text = response.into_string()?;
            Err(anyhow::anyhow!("Failed to get all notes: {}", error_text))
        }
    }
}

// Define the ErgoBoxDetails struct for parsing box data from the Ergo node
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ErgoBoxDetails {
    #[serde(alias = "boxId")]
    pub box_id: String,
    pub value: u64,
    #[serde(alias = "ergoTree")]
    pub ergo_tree: String,
    pub assets: Vec<Token>,
    #[serde(alias = "additionalRegisters")]
    pub additional_registers: std::collections::HashMap<String, String>,
    #[serde(alias = "creationHeight")]
    pub creation_height: u32,
    #[serde(alias = "transactionId")]
    pub transaction_id: String,
    pub index: u16,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Token {
    #[serde(alias = "tokenId")]
    pub token_id: String,
    pub amount: u64,
}

// Define the SerializableIouNoteWithAge struct outside of the impl block
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct SerializableIouNoteWithAge {
    pub issuer_pubkey: String, // Changed from issuer_pubkey to match server response
    pub recipient_pubkey: String, // Changed from recipient_pubkey to match server response
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

#[cfg(test)]
mod v1_redemption_tombstone_tests {
    use super::*;

    fn assert_retired<T>(result: Result<T>) {
        assert_eq!(
            result.err().expect("v1 call must fail").to_string(),
            V1_REDEMPTION_RETIRED
        );
    }

    #[tokio::test]
    async fn every_v1_client_proof_build_sign_and_submit_call_fails_before_network_io() {
        let client = TrackerClient::new("http://127.0.0.1:1".to_string());

        assert_retired(
            client
                .request_tracker_signature("issuer", "receiver", 1, 1, false)
                .await,
        );
        assert_retired(client.get_tracker_proof("issuer", "receiver").await);
        assert_retired(client.get_reserve_proof("issuer", "receiver", 1, 1).await);
        assert_retired(client.prepare_redemption("issuer", "receiver", 1).await);
        assert_retired(
            client
                .redemption_build(RedemptionBuildRequest {
                    issuer_pubkey: "issuer".to_string(),
                    recipient_pubkey: "receiver".to_string(),
                    amount: 1,
                    timestamp: 1,
                    issuer_signature: "signature".to_string(),
                    emergency: false,
                    tracker_box_id: None,
                })
                .await,
        );
        assert_retired(client.redemption_submit(serde_json::json!({})).await);
    }
}
