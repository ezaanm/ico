# A _beautiful_ yet **simple** ICO Contract

A Basic ICO contract that can be deployed on cosmos chains like Terra, handles basic logic of accepting funds from contributors during when funding is open, closing funding with proper authentication or when target goal has been reached, and minting and transferring custom derivative tokens to funders. Built with rust and cosmwasm-std 0.14.

## Token types
Defined in the ```InstantiateMsg``` when the contract is instantiated.

This contract only accepts coins of type ```fundraise_denom```, and taps into [CosmWasm cw20-base](https://github.com/CosmWasm/cosmwasm-plus/tree/master/contracts/cw20-base) which implements the [CosmWasm cw20](https://github.com/CosmWasm/cosmwasm-plus/tree/master/packages/cw20) spec to mint and transfer derivative tokens named by the instantiator to funders when funding is closed. 

## Message Types
### InstantiateMsg
```
pub struct InstantiateMsg {
    /// goal fundraise amount
    pub fundraise_goal: Uint128,
    /// conversion ratio of fundraise_denom:derivative_token 
    pub base_conv_ratio: Decimal,
    /// denom of coins sent to this contract for fundraising
    pub fundraise_denom: String,

    /// name of the derivative token
    pub name: String,
    /// symbol / ticker of the derivative token
    pub symbol: String,
    /// decimal places of the derivative token (for UI)
    pub decimals: u8,
}
```
ICO initial state is defined along with the name and symbol of the derivative tokens that will be minted and sent to funders when funding is closed. Sets initial derivative token supply to 0 and fundraising to open.

### ExecuteMsg
```
AddFunds {}
```
Ensuring only ```fundraise_denom``` tokens are sent, this creates an account for the sender and adds all tokens sent with the call to their account. Can be called multiple times by the same or new senders. Can only be called while fundraising is set to open.

```
CloseFundraise {}
```
Can be called by any user once ```fundraise_bal >= fundraise_goal``` to close fundraising and trigger a set of callbacks that mints and sends derivative tokens to funders. Can be called by contract owner to early close fundraising at any time.

```
_SendTokens{},
```
Callback called by the contract itself to mint the required number of derivative tokens accoridng to how much was deposited and the base_conv_ratio set when instantiating the contract. Fires off multiple Transfers after minting derivative tokens.

```
Transfer {
  recipient: HumanAddr,
  amount: Uint128,
 }
 ```
Transfer is a base message to move tokens to another account without triggering actions, here I use it as a callback function from _SendTokens()_ to send derivative tokens to all funders for the required amount.

### QueryMsg
```
FundraiseInfo {}
```
Returns status of ICO: fundraise_goal, fundraise_bal, and other basic information.

```
StakedInfo {}
```
Returns the total amount contributed along with an array of all contributers and their amount contributed.

```
Balance { address: HumanAddr }
```
Returns the current balance of derivative tokens for the given address, 0 if unset.
    
```
TokenInfo {}
```
Returns metadata on the derivate token - name, decimals, supply, etc.

## Testing
```cargo test``` will fire off a set of tests defined in contract.rs

