#[cfg(not(feature = "library"))]
use cosmwasm_std::entry_point;
use cosmwasm_std::{
    attr, to_binary, Binary, Deps, DepsMut, Env, HumanAddr,
    MessageInfo, Response, StdResult, WasmMsg, Uint128, Decimal
};

use cw2::set_contract_version;
use cw20_base::state::{MinterData, TokenInfo, TOKEN_INFO};
use cw20_base::contract::{execute_mint, execute_transfer, query_balance, query_token_info};

use crate::error::ContractError;
use crate::msg::{
    ExecuteMsg, InstantiateMsg, FundraiseInfoResponse, ListResponse, QueryMsg,
};

use crate::state::{ICOInfo, Fundraiser, ICO, Rate};

// version info for migration info
const CONTRACT_NAME: &str = "crates.io:icov3";
const CONTRACT_VERSION: &str = env!("CARGO_PKG_VERSION");

#[cfg_attr(not(feature = "library"), entry_point)]
pub fn instantiate(
    deps: DepsMut,
    env: Env,
    info: MessageInfo,
    msg: InstantiateMsg,
) -> StdResult<Response> {
    set_contract_version(deps.storage, CONTRACT_NAME, CONTRACT_VERSION)?;

    //flip to be den/num to support multiplication
    let base_ratio = Decimal::from_ratio(msg.base_conv_ratio_den, msg.base_conv_ratio_num);

    let ico_rates:Vec<Rate> = match msg.rates {
        Some(init_rates) => {
            let mut rates:Vec<Rate> = init_rates.iter().map(|x| Rate{min: x.min, ratio: Decimal::from_ratio(x.ratio_den, x.ratio_num)}).collect();
            rates.push(Rate{min: Uint128(0), ratio: base_ratio});
            rates.sort_by(|a, b| b.min.cmp(&a.min));
            rates
        },
        None => vec![Rate{min: Uint128(0), ratio: base_ratio}],
    };
    
    //setup ICO base information
    let ico_info = ICOInfo {
        fundraise_goal: msg.fundraise_goal,
        fundraise_bal: Uint128(0),
        base_conv_ratio: base_ratio,
        owner: deps.api.canonical_address(&info.sender)?,
        fundraising_open: true,
        fundraise_denom: msg.fundraise_denom,
        fundraisers: vec![],
        rates: ico_rates,
    };

    ICO.save(deps.storage, &ico_info)?;

    // store token info using cw20-base format
    let token_info = TokenInfo {
        name: msg.name,
        symbol: msg.symbol,
        decimals: msg.decimals,
        total_supply: Uint128(0),
        // set self as minter, so we can properly execute mint and burn
        mint: Some(MinterData {
            minter: deps.api.canonical_address(&env.contract.address)?,
            cap: None,
        }),
    };
    TOKEN_INFO.save(deps.storage, &token_info)?;

    Ok(Response::default())
}

#[cfg_attr(not(feature = "library"), entry_point)]
pub fn execute(
    deps: DepsMut,
    env: Env,
    info: MessageInfo,
    msg: ExecuteMsg,
) -> Result<Response, ContractError> {
    match msg {
        ExecuteMsg::AddFunds {} => execute_add_funds(deps, info),
        ExecuteMsg::CloseFundraise {} => execute_close_fundraise(deps, env, &info.sender),
        ExecuteMsg::_SendTokens {} => _send_tokens(deps, env, info),

        ExecuteMsg::Transfer { recipient, amount } => {
            Ok(execute_transfer(deps, env, info, recipient, amount)?)
        }
    }
}

pub fn execute_add_funds(
    deps: DepsMut,
    info: MessageInfo,
) -> Result<Response, ContractError> {
    let mut ico_info = ICO.load(deps.storage)?;
    if ico_info.fundraising_open == false {
        return Err(ContractError::FundraiseClosed {});
    }

    let payment = info
        .funds
        .iter()
        .find(|x| x.denom == ico_info.fundraise_denom && !x.amount.is_zero())
        .ok_or_else(|| ContractError::EmptyBalance {})?;

    let index = ico_info.fundraisers.iter().enumerate().find_map(|(i, exist)| {
        if exist.source == info.sender {
            Some(i)
        } else {
            None
        }
    });

    match index {
        Some(idx) => ico_info.fundraisers[idx].balance += payment.amount,
        None => ico_info.fundraisers.push(Fundraiser {
            source: info.sender.clone(),
            balance: payment.amount,
        }),
    }
    
    ico_info.fundraise_bal += payment.amount;
    ICO.save(deps.storage, &ico_info)?;

    let res = Response {
        attributes: vec![attr("action", "add_funds"), attr("id", info.sender.as_str())],
        ..Response::default()
    };
    Ok(res)
}

pub fn execute_close_fundraise(
    deps: DepsMut,
    env: Env,
    sender: &HumanAddr,
) -> Result<Response, ContractError> {

    let mut ico_info = ICO.load(deps.storage)?;
    let canonical = deps.api.canonical_address(sender)?;

    if ico_info.fundraising_open {
        if canonical == ico_info.owner || ico_info.fundraise_bal >= ico_info.fundraise_goal {
            ico_info.fundraising_open = false;
            ICO.save(deps.storage, &ico_info)?;
            
            //fundraising is closed, send callback to send everyone their cw20 tokens
            let contract_addr = env.contract.address;
            let msg = to_binary(&ExecuteMsg::_SendTokens {})?;

            let res = Response {
                submessages: vec![],
                messages: vec![
                    WasmMsg::Execute {
                        contract_addr,
                        msg,
                        send: vec![],
                    }
                    .into(),
                ],
                attributes: vec![attr("action", "close_fundraise")],
                data: None,
            };
            return Ok(res);

        }
    } 

    return Err(ContractError::FundraiseClosed {});
}

pub fn _send_tokens(
    mut deps: DepsMut,
    env: Env,
    info: MessageInfo,
) -> Result<Response, ContractError> {
    let human_contract_address = env.contract.address.clone();

    if info.sender != human_contract_address {
        return Err(ContractError::Unauthorized {});
    }

    let ico_info = ICO.load(deps.storage)?;

    //iter through fundraisers and send them right number of tokens
    let mut messages = vec![];
    let mut to_mint = Uint128(0);

    for f in &ico_info.fundraisers {
        let f_rate = ico_info.rates.iter().find(|r| f.balance >= r.min);
        let ratio = match f_rate {
            Some(r) => r.ratio,
            None => ico_info.base_conv_ratio,
        };

        let recieves = f.balance * ratio;
        
        to_mint += recieves;

        let binary_msg = to_binary(&ExecuteMsg::Transfer {
            recipient: f.source.clone(),
            amount: recieves,
        })?;

        let wasm_exec = WasmMsg::Execute {
                contract_addr: human_contract_address.clone(),
                msg: binary_msg,
                send: vec![],
            }.into();

        messages.push(wasm_exec);
    }

    //mint required tokens to the contract itself
    // call into cw20-base to mint the token, call as self as no one else is allowed
    let sub_info = MessageInfo {
        sender: human_contract_address.clone(),
        funds: vec![],
    };

    execute_mint(deps.branch(), env, sub_info, human_contract_address.clone(), to_mint)?;

    //send
    let res = Response {
        submessages: vec![],
        messages,
        attributes: vec![attr("action", "transfer")],
        data: None,
    };

    Ok(res)
}

#[cfg_attr(not(feature = "library"), entry_point)]
pub fn query(deps: Deps, _env: Env, msg: QueryMsg) -> StdResult<Binary> {
    match msg {
        QueryMsg::TokenInfo {} => to_binary(&query_token_info(deps)?),
        QueryMsg::Balance { address } => to_binary(&query_balance(deps, address)?),
        QueryMsg::FundraiseInfo {} => to_binary(&query_fundraise(deps)?),
        QueryMsg::StakedInfo {} => to_binary(&query_staked(deps)?),

    }
}

pub fn query_fundraise(deps: Deps) -> StdResult<FundraiseInfoResponse> {
    let ico_info = ICO.load(deps.storage)?;

    let res = FundraiseInfoResponse {
        fundraise_goal: ico_info.fundraise_goal,
        fundraise_bal: ico_info.fundraise_bal,
        base_conv_ratio: ico_info.base_conv_ratio,
        owner: deps.api.human_address(&ico_info.owner)?,
        fundraising_open: ico_info.fundraising_open,
        fundraise_denom: ico_info.fundraise_denom,
        rates: ico_info.rates,
    };
    Ok(res)
}

pub fn query_staked(deps: Deps) -> StdResult<ListResponse> {
    let ico_info = ICO.load(deps.storage)?;

    let res = ListResponse {
        total_staked: ico_info.fundraise_bal,
        fundraisers: ico_info.fundraisers, 
    };
    Ok(res)
}


#[cfg(test)]
mod tests {
    use cosmwasm_std::testing::{mock_dependencies, mock_env, mock_info, MOCK_CONTRACT_ADDR};
    use cosmwasm_std::{coins, CosmosMsg, Uint128, Decimal};
    use cw20::{TokenInfoResponse, BalanceResponse};
    use crate::msg::RateInit;

    use super::*;

    #[test]
    fn can_add_luna() {
        let mut deps = mock_dependencies(&[]);

        // instantiate a contract
        let instantiate_msg = InstantiateMsg {
            fundraise_goal: Uint128(100),
            base_conv_ratio_num: Uint128(1),
            base_conv_ratio_den: Uint128(1),
            fundraise_denom: "uluna".to_string(),
            name: "Shark Coin".to_string(),
            symbol: "ushark".to_string(),
            decimals: 0,
            rates: None,
        };

        let info = mock_info(&HumanAddr::from("god"), &[]);
        let res = instantiate(deps.as_mut(), mock_env(), info, instantiate_msg).unwrap();
        assert_eq!(0, res.messages.len());

        //add funds
        let sender = HumanAddr::from("casper");
        let balance = coins(5, "uluna");
        let info = mock_info(&sender, &balance);
        let msg = ExecuteMsg::AddFunds {};
        let res = execute(deps.as_mut(), mock_env(), info, msg).unwrap();
        assert_eq!(0, res.messages.len());
        assert_eq!(attr("action", "add_funds"), res.attributes[0]);

        //add funds 2
        let sender = HumanAddr::from("marcel");
        let balance = coins(5, "uluna");
        let info = mock_info(&sender, &balance);
        let msg = ExecuteMsg::AddFunds {};
        let res = execute(deps.as_mut(), mock_env(), info, msg).unwrap();
        assert_eq!(0, res.messages.len());
        assert_eq!(attr("action", "add_funds"), res.attributes[0]);

        let qfund = query_fundraise(deps.as_ref()).unwrap();
        assert_eq!(
            qfund,
            FundraiseInfoResponse {
                fundraise_goal: Uint128(100),
                fundraise_bal: Uint128(10),
                base_conv_ratio: Decimal::one(),
                owner: HumanAddr::from("god"),
                fundraising_open: true,
                fundraise_denom: "uluna".to_string(),
                rates: vec![Rate {min: Uint128(0), ratio: Decimal::one()}],
            }
        );

        //do the staking accounts exist
        let qstaked = query_staked(deps.as_ref()).unwrap();
        assert!(qstaked.fundraisers.iter().any(|f| f.balance == Uint128(5) && f.source == HumanAddr::from("casper")));
        assert!(qstaked.fundraisers.iter().any(|f| f.balance == Uint128(5) && f.source == HumanAddr::from("marcel")));
    }

    #[test]
    fn token_created() {
        let mut deps = mock_dependencies(&[]);

        // instantiate a contract
        let instantiate_msg = InstantiateMsg {
            fundraise_goal: Uint128(100),
            base_conv_ratio_num: Uint128(1),
            base_conv_ratio_den: Uint128(1),
            fundraise_denom: "uluna".to_string(),
            name: "Shark Coin".to_string(),
            symbol: "ushark".to_string(),
            decimals: 0,
            rates: None,
        };

        let info = mock_info(&HumanAddr::from("god"), &[]);
        let res = instantiate(deps.as_mut(), mock_env(), info, instantiate_msg).unwrap();
        assert_eq!(0, res.messages.len());

        //check if token exists
        let qtoken = query_token_info(deps.as_ref()).unwrap();
        assert_eq!(
            qtoken,
            TokenInfoResponse {
                name: "Shark Coin".to_string(),
                symbol: "ushark".to_string(),
                decimals: 0,
                total_supply: Uint128(0),
            }
        );
    }

    #[test]
    fn close_and_send() {
        let mut deps = mock_dependencies(&[]);

        // instantiate a contract
        let instantiate_msg = InstantiateMsg {
            fundraise_goal: Uint128(100),
            base_conv_ratio_num: Uint128(1),
            base_conv_ratio_den: Uint128(1),
            fundraise_denom: "uluna".to_string(),
            name: "Shark Coin".to_string(),
            symbol: "ushark".to_string(),
            decimals: 0,
            rates: None,
        };

        let info = mock_info(&HumanAddr::from("god"), &[]);
        let res = instantiate(deps.as_mut(), mock_env(), info, instantiate_msg).unwrap();
        assert_eq!(0, res.messages.len());

        //add funds
        let sender = HumanAddr::from("casper");
        let balance = coins(100, "uluna");
        let info = mock_info(&sender, &balance);
        let msg = ExecuteMsg::AddFunds {};
        let res = execute(deps.as_mut(), mock_env(), info, msg).unwrap();
        assert_eq!(0, res.messages.len());
        assert_eq!(attr("action", "add_funds"), res.attributes[0]);

        //add funds
        let sender = HumanAddr::from("marcel");
        let balance = coins(50, "uluna");
        let info = mock_info(&sender, &balance);
        let msg = ExecuteMsg::AddFunds {};
        let res = execute(deps.as_mut(), mock_env(), info, msg).unwrap();
        assert_eq!(0, res.messages.len());
        assert_eq!(attr("action", "add_funds"), res.attributes[0]);

        //close fundraise
        let sender = HumanAddr::from("casper");
        let info = mock_info(&sender, &[]);
        let msg = ExecuteMsg::CloseFundraise {};
        let res = execute(deps.as_mut(), mock_env(), info, msg).unwrap();
        assert_eq!(1, res.messages.len());
        assert_eq!(attr("action", "close_fundraise"), res.attributes[0]);
        
        let sendmsg = &res.messages[0];
        match sendmsg {
            CosmosMsg::Wasm(WasmMsg::Execute { contract_addr, msg:_, send }) => {
                assert_eq!(send, &[]);
                assert_eq!(contract_addr, &HumanAddr::from(MOCK_CONTRACT_ADDR));
            }
            _ => panic!("Unexpected message: {:?}", sendmsg),
        }

        //fake callback
        let info = mock_info(MOCK_CONTRACT_ADDR, &[]);
        let msg = ExecuteMsg::_SendTokens {};
        let res = execute(deps.as_mut(), mock_env(), info, msg).unwrap();

        //check if 150 token was minted
        let qtoken = query_token_info(deps.as_ref()).unwrap();
        assert_eq!(
            qtoken,
            TokenInfoResponse {
                name: "Shark Coin".to_string(),
                symbol: "ushark".to_string(),
                decimals: 0,
                total_supply: Uint128(150),
            }
        );

        //check to see if contract was given minted coins
        let qbal = query_balance(deps.as_ref(), HumanAddr::from(MOCK_CONTRACT_ADDR)).unwrap();
        assert_eq!(
            qbal,
            BalanceResponse {
                balance: Uint128(150)
            }
        );

        //check if 2 transfers are sent
        assert_eq!(2, res.messages.len());
        assert_eq!(attr("action", "transfer"), res.attributes[0]);

        //fake transfers
        let info = mock_info(MOCK_CONTRACT_ADDR, &[]);
        let msg = ExecuteMsg::Transfer {
            amount: Uint128(100),
            recipient: HumanAddr::from("casper"),
        };
        let _ = execute(deps.as_mut(), mock_env(), info.clone(), msg).unwrap();

        let msg = ExecuteMsg::Transfer {
            amount: Uint128(50),
            recipient: HumanAddr::from("marcel"),
        };
        let _ = execute(deps.as_mut(), mock_env(), info, msg).unwrap();

        //check their balance
        let qbal = query_balance(deps.as_ref(), HumanAddr::from("casper")).unwrap();
        assert_eq!(
            qbal,
            BalanceResponse {
                balance: Uint128(100)
            }
        );

        let qbal = query_balance(deps.as_ref(), HumanAddr::from("marcel")).unwrap();
        assert_eq!(
            qbal,
            BalanceResponse {
                balance: Uint128(50)
            }
        );

    }

    #[test]
    fn rate_sort() {
        let mut rate_arr:Vec<Rate> = vec![];

        let r1 = Rate {min: Uint128(5), ratio: Decimal::percent(120)};
        let r2 = Rate {min: Uint128(10), ratio: Decimal::percent(200)};

        rate_arr.push(r1);
        rate_arr.push(r2);

        assert_eq!(rate_arr[0], Rate {min: Uint128(5), ratio: Decimal::percent(120)});

        rate_arr.sort_by(|a, b| b.min.cmp(&a.min));

        assert_eq!(rate_arr[0], Rate {min: Uint128(10), ratio: Decimal::percent(200)});
    }

    #[test]
    fn recieve_proper_rate() {
        let mut deps = mock_dependencies(&[]);

        // instantiate a contract
        let instantiate_msg = InstantiateMsg {
            fundraise_goal: Uint128(10),
            base_conv_ratio_num: Uint128(1),
            base_conv_ratio_den: Uint128(1),
            fundraise_denom: "uluna".to_string(),
            name: "Shark Coin".to_string(),
            symbol: "ushark".to_string(),
            decimals: 0,
            rates: Some(vec![RateInit {min: Uint128(10), ratio_num: Uint128(1), ratio_den: Uint128(2)}, 
                             RateInit {min: Uint128(20), ratio_num: Uint128(1), ratio_den: Uint128(3)}]),
        };

        let info = mock_info(&HumanAddr::from("god"), &[]);
        let res = instantiate(deps.as_mut(), mock_env(), info, instantiate_msg).unwrap();
        assert_eq!(0, res.messages.len());

        //add funds
        let sender = HumanAddr::from("casper");
        let balance = coins(5, "uluna");
        let info = mock_info(&sender, &balance);
        let msg = ExecuteMsg::AddFunds {};
        let res = execute(deps.as_mut(), mock_env(), info, msg).unwrap();
        assert_eq!(0, res.messages.len());
        assert_eq!(attr("action", "add_funds"), res.attributes[0]);

        //add funds
        let sender = HumanAddr::from("marcel");
        let balance = coins(10, "uluna");
        let info = mock_info(&sender, &balance);
        let msg = ExecuteMsg::AddFunds {};
        let res = execute(deps.as_mut(), mock_env(), info, msg).unwrap();
        assert_eq!(0, res.messages.len());
        assert_eq!(attr("action", "add_funds"), res.attributes[0]);

        //add funds
        let sender = HumanAddr::from("kanye");
        let balance = coins(50, "uluna");
        let info = mock_info(&sender, &balance);
        let msg = ExecuteMsg::AddFunds {};
        let res = execute(deps.as_mut(), mock_env(), info, msg).unwrap();
        assert_eq!(0, res.messages.len());
        assert_eq!(attr("action", "add_funds"), res.attributes[0]);

        //close fundraise
        let sender = HumanAddr::from("casper");
        let info = mock_info(&sender, &[]);
        let msg = ExecuteMsg::CloseFundraise {};
        let res = execute(deps.as_mut(), mock_env(), info, msg).unwrap();
        assert_eq!(1, res.messages.len());
        assert_eq!(attr("action", "close_fundraise"), res.attributes[0]);
        
        let sendmsg = &res.messages[0];
        match sendmsg {
            CosmosMsg::Wasm(WasmMsg::Execute { contract_addr, msg:_, send }) => {
                assert_eq!(send, &[]);
                assert_eq!(contract_addr, &HumanAddr::from(MOCK_CONTRACT_ADDR));
            }
            _ => panic!("Unexpected message: {:?}", sendmsg),
        }

        //fake callback
        let info = mock_info(MOCK_CONTRACT_ADDR, &[]);
        let msg = ExecuteMsg::_SendTokens {};
        let _ = execute(deps.as_mut(), mock_env(), info, msg).unwrap();

        //check if 5 + 10*2 + 50*3 = 175 tokens were minted
        let qtoken = query_token_info(deps.as_ref()).unwrap();
        assert_eq!(
            qtoken,
            TokenInfoResponse {
                name: "Shark Coin".to_string(),
                symbol: "ushark".to_string(),
                decimals: 0,
                total_supply: Uint128(175),
            }
        );
    }
}
