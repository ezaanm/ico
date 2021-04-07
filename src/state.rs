use schemars::JsonSchema;
use serde::{Deserialize, Serialize};

use cosmwasm_std::{CanonicalAddr, HumanAddr, Decimal, Uint128};
use cw_storage_plus::Item;

#[derive(Serialize, Deserialize, Clone, PartialEq, JsonSchema, Debug)]
pub struct ICOInfo {
    //basic ICO info
    
    /// value of the goal we want to raise
    pub fundraise_goal: Uint128,
    ///value of how much we have raised so far
    pub fundraise_bal: Uint128,
    ///initial ratio of LUNA:ASSET 
    pub base_conv_ratio: Decimal,
    /// who created this ICO
    pub owner: CanonicalAddr,
    /// If fundraising is open to contributions or not
    pub fundraising_open: bool,
    /// Denom of token accepted to fundraise with
    pub fundraise_denom: String,
    ///list of contributors and how much they have sent
    pub fundraisers: Vec<Fundraiser>,
}

pub const ICO: Item<ICOInfo> = Item::new("ico");

#[derive(Serialize, Deserialize, Clone, PartialEq, JsonSchema, Debug)]
pub struct Fundraiser {
    /// who sent Luna
    pub source: HumanAddr,

    /// Balance of Native tokens sent to ICO
    pub balance: Uint128,
}
