use schemars::JsonSchema;
use serde::{Deserialize, Serialize};

use cosmwasm_std::{HumanAddr, Uint128, Decimal};

use crate::state::{Fundraiser, Rate};

#[derive(Serialize, Deserialize, Clone, Debug, PartialEq, JsonSchema)]
pub struct InstantiateMsg {
    /// goal fundraise amount
    pub fundraise_goal: Uint128,
    /// conversion ratio of fundraise_denom:derivative_token 
    pub base_conv_ratio: Decimal,
    /// denom of coins sent to this contract for fundraising
    pub fundraise_denom: String,
    /// nullable field of Rates
    pub rates: Option<Vec<Rate>>,

    /// name of the derivative token
    pub name: String,
    /// symbol / ticker of the derivative token
    pub symbol: String,
    /// decimal places of the derivative token (for UI)
    pub decimals: u8,
}

#[derive(Serialize, Deserialize, Clone, Debug, PartialEq, JsonSchema)]
#[serde(rename_all = "snake_case")]
pub enum ExecuteMsg {
    CloseFundraise {},
    AddFunds {},
    _SendTokens{},
    
    /// Implements CW20. Transfer is a base message to move tokens to another account without triggering actions
    Transfer {
        recipient: HumanAddr,
        amount: Uint128,
    },
}

#[derive(Serialize, Deserialize, Clone, Debug, PartialEq, JsonSchema)]
#[serde(rename_all = "snake_case")]
pub enum QueryMsg {
    /// Shows how much we have raised so far and our goal
    FundraiseInfo {},
    
    /// Shows how much has been staked for each address
    StakedInfo {},

    /// Implements CW20. Returns the current balance of the given address, 0 if unset.
    Balance { address: HumanAddr },
    /// Implements CW20. Returns metadata on the contract - name, decimals, supply, etc.
    TokenInfo {},
}

#[derive(Serialize, Deserialize, Clone, PartialEq, JsonSchema, Debug)]
pub struct ListResponse {
    /// total staked
    pub total_staked: Uint128,
    /// list all stakers and how much
    pub fundraisers: Vec<Fundraiser>,
}

#[derive(Serialize, Deserialize, Clone, PartialEq, JsonSchema, Debug)]
pub struct FundraiseInfoResponse {
    /// value of the goal we want to raise
    pub fundraise_goal: Uint128,
    ///value of how much we have raised so far
    pub fundraise_bal: Uint128,
    ///initial ratio of LUNA:ASSET 
    pub base_conv_ratio: Decimal,
    /// who created this ICO
    pub owner: HumanAddr,
    /// If fundraising is open to contributions or not
    pub fundraising_open: bool,
    /// Denom of token accepted to fundraise with
    pub fundraise_denom: String,
    /// rates offered
    pub rates: Vec<Rate>,
}
