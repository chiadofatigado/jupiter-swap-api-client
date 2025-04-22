use crate::{
    quote::QuoteResponse,
    serde_helpers::{field_as_string, option_field_as_string},
};
use anyhow::Context; // Add context for better error messages
use base64::{engine::general_purpose::STANDARD, Engine as _}; // Import base64 engine
use serde::{Deserialize, Serialize};
use solana_sdk::{instruction as solana_instruction, pubkey::Pubkey}; // Use sdk types directly
use std::str::FromStr;
use serde_with::{serde_as, base64::Base64}; // Keep for SwapResponse, add Base64 helper

#[derive(Serialize, Deserialize, Clone, Debug)]
#[serde(rename_all = "camelCase")]
pub struct SwapRequest {
    #[serde(with = "field_as_string")]
    pub user_public_key: String,
    pub wrap_and_unwrap_sol: bool,
    pub auto_create_out_ata: bool,
    pub use_shared_accounts: bool,
    #[serde(with = "option_field_as_string")]
    pub fee_account: Option<String>,
    pub compute_unit_price_micro_lamports: Option<u64>,
    pub as_legacy_transaction: bool,
    pub use_token_ledger: bool,
    #[serde(with = "option_field_as_string")]
    pub destination_token_account: Option<String>,
    pub quote_response: QuoteResponse,
}

#[derive(Serialize, Deserialize, Debug, Clone)]
#[serde(rename_all = "camelCase")]
#[serde_as]
pub struct SwapResponse {
    #[serde_as(as = "Base64")] // Use serde_with helper
    pub swap_transaction: Vec<u8>,
    pub last_valid_block_height: u64,
    pub priorization_fee_lamports: u64,
}

// Public response struct using solana_sdk types
#[derive(Debug, Clone)]
pub struct SwapIxResponse {
    pub token_ledger_instruction: Option<solana_instruction::Instruction>,
    pub compute_budget_instructions: Option<Vec<solana_instruction::Instruction>>,
    pub setup_instructions: Option<Vec<solana_instruction::Instruction>>,
    pub swap_instruction: solana_instruction::Instruction,
    pub cleanup_instructions: Option<Vec<solana_instruction::Instruction>>,
    pub address_lookup_table_addresses: Option<Vec<String>>, // Keep as String for now
}

// Internal struct for deserialization matching the JSON structure
// This struct should be used internally by the client when deserializing the API response.
#[derive(Deserialize, Debug, Clone)]
#[serde(rename_all = "camelCase")]
pub(crate) struct SwapIxResponseInternal {
    token_ledger_instruction: Option<InstructionInternal>,
    compute_budget_instructions: Option<Vec<InstructionInternal>>,
    setup_instructions: Option<Vec<InstructionInternal>>,
    swap_instruction: InstructionInternal,
    cleanup_instructions: Option<Vec<InstructionInternal>>,
    address_lookup_table_addresses: Option<Vec<String>>,
}

// Internal struct for deserialization
#[derive(Deserialize, Debug, Clone)]
#[serde(rename_all = "camelCase")]
struct InstructionInternal {
    program_id: String,
    data: Option<String>,
    accounts: Option<Vec<AccountMetaInternal>>,
}

// Internal struct for deserialization
#[derive(Deserialize, Debug, Clone)]
#[serde(rename_all = "camelCase")]
struct AccountMetaInternal {
    pubkey: String,
    is_signer: Option<bool>,
    is_writable: Option<bool>,
}

// Conversion from internal AccountMetaInternal to solana_sdk::instruction::AccountMeta
impl TryFrom<&AccountMetaInternal> for solana_instruction::AccountMeta {
    type Error = anyhow::Error;
    fn try_from(m: &AccountMetaInternal) -> Result<Self, Self::Error> {
        Ok(Self {
            pubkey: Pubkey::from_str(&m.pubkey)
                .with_context(|| format!("Failed to parse pubkey string: {}", m.pubkey))?,
            is_signer: m.is_signer.unwrap_or(false),
            is_writable: m.is_writable.unwrap_or(false),
        })
    }
}

// Conversion from internal InstructionInternal to solana_sdk::instruction::Instruction
impl TryFrom<&InstructionInternal> for solana_instruction::Instruction {
    type Error = anyhow::Error;
    fn try_from(m: &InstructionInternal) -> Result<Self, Self::Error> {
        Ok(Self {
            program_id: Pubkey::from_str(&m.program_id)
                .with_context(|| format!("Failed to parse program_id string: {}", m.program_id))?,
            data: match m.data.as_ref() {
                Some(d) => STANDARD
                    .decode(d)
                    .with_context(|| format!("Failed to decode base64 data for program {}", m.program_id))?,
                None => vec![],
            },
            accounts: match m.accounts.as_ref() {
                Some(accs) => accs
                    .iter()
                    .map(|a| a.try_into())
                    .collect::<anyhow::Result<Vec<solana_instruction::AccountMeta>>>()
                    .with_context(|| format!("Failed to convert accounts for program {}", m.program_id))?,
                None => vec![],
            },
        })
    }
}

// Conversion from the internal deserialization struct to the public struct
// The library's API fetching function should deserialize into SwapIxResponseInternal
// and then call this conversion.
impl TryFrom<SwapIxResponseInternal> for SwapIxResponse {
    type Error = anyhow::Error;
    fn try_from(internal: SwapIxResponseInternal) -> Result<Self, Self::Error> {
        let convert_instructions = |instructions: Option<Vec<InstructionInternal>>, name: &str| -> anyhow::Result<Option<Vec<solana_instruction::Instruction>>> {
            match instructions {
                Some(internal_ixs) => {
                    let converted = internal_ixs
                        .iter()
                        .map(|ix| ix.try_into())
                        .collect::<anyhow::Result<Vec<_>>>()
                        .with_context(|| format!("Failed to convert {} instructions", name))?;
                    Ok(Some(converted))
                }
                None => Ok(None),
            }
        };

        let convert_instruction = |instruction: Option<InstructionInternal>, name: &str| -> anyhow::Result<Option<solana_instruction::Instruction>> {
             match instruction {
                Some(internal_ix) => {
                    let converted = (&internal_ix).try_into()
                        .with_context(|| format!("Failed to convert {} instruction", name))?;
                    Ok(Some(converted))
                }
                None => Ok(None),
            }
        };

        Ok(Self {
            token_ledger_instruction: convert_instruction(internal.token_ledger_instruction, "token_ledger")?,
            compute_budget_instructions: convert_instructions(internal.compute_budget_instructions, "compute_budget")?,
            setup_instructions: convert_instructions(internal.setup_instructions, "setup")?,
            swap_instruction: (&internal.swap_instruction)
                .try_into()
                .context("Failed to convert swap_instruction")?,
            cleanup_instructions: convert_instructions(internal.cleanup_instructions, "cleanup")?,
            address_lookup_table_addresses: internal.address_lookup_table_addresses,
        })
    }
}
