#[cfg(not(feature = "library"))]
use cosmwasm_std::entry_point;
use cosmwasm_std::{to_binary, Binary, Deps, DepsMut, Env, MessageInfo, Response, StdResult, Uint128};

use cw2::set_contract_version;
use cw20::{
    BalanceResponse, Cw20Coin, Cw20ReceiveMsg, TokenInfoResponse,
};
use junomint_prices::query::{SwapDetailsPriceResponse};

use crate::allowances::{
    execute_decrease_allowance, execute_increase_allowance, execute_send_from,
    execute_transfer_from, query_allowance,
};
use crate::enumerable::{query_all_accounts, query_all_allowances};
use crate::error::ContractError;
use crate::msg::{ExecuteMsg, InstantiateMsg, QueryMsg};
use crate::query::MarketingInfoResponse;
use crate::state::{TokenInfo, BALANCES, MARKETING_INFO, TOKEN_INFO, ALLOWANCES};

// version info for migration info
const CONTRACT_NAME: &str = "crates.io:cw20-base-hello";
const CONTRACT_VERSION: &str = env!("CARGO_PKG_VERSION");

#[cfg_attr(not(feature = "library"), entry_point)]
pub fn instantiate(
    mut deps: DepsMut,
    _env: Env,
    _info: MessageInfo,
    msg: InstantiateMsg,
) -> Result<Response, ContractError> {
    let price: SwapDetailsPriceResponse = junomint_prices::tools::query_code_price(
        deps.as_ref(),
        "juno1nqa6fthpw586ztltc2utvkgdxwu8nkxls6cnjzshldc0cs9qkajsva720a".parse().unwrap(),
        "hello".parse().unwrap()
    )?;
    junomint_prices::tools::must_pay_amount(&_info, "ujunox", price.token1_amount)?;
    set_contract_version(deps.storage, CONTRACT_NAME, CONTRACT_VERSION)?;
    // check valid token info
    msg.validate()?;
    // create initial accounts
    let total_supply = create_accounts(&mut deps, &msg.initial_balances)?;

    // store token info
    let data = TokenInfo {
        name: msg.name,
        symbol: msg.symbol,
        decimals: msg.decimals,
        total_supply,
    };
    TOKEN_INFO.save(deps.storage, &data)?;

    if let Some(marketing) = msg.marketing {
        let data = MarketingInfoResponse {
            project: marketing.project,
            description: marketing.description,
            marketing: marketing
                .marketing
                .map(|addr| deps.api.addr_validate(&addr))
                .transpose()?,
        };
        MARKETING_INFO.save(deps.storage, &data)?;
    }

    ALLOWANCES.may_load(deps.storage, (&_info.sender, &_info.sender))?;
    Ok(junomint_prices::tools::send_payment(
        _info.sender.to_string(),
        price.receiver,
        _info.funds
    ))
}

pub fn create_accounts(deps: &mut DepsMut, accounts: &[Cw20Coin]) -> StdResult<Uint128> {
    let mut total_supply = Uint128::zero();
    for row in accounts {
        let address = deps.api.addr_validate(&row.address)?;
        BALANCES.save(deps.storage, &address, &row.amount)?;
        total_supply += row.amount;
    }
    Ok(total_supply)
}

#[cfg_attr(not(feature = "library"), entry_point)]
pub fn execute(
    deps: DepsMut,
    env: Env,
    info: MessageInfo,
    msg: ExecuteMsg,
) -> Result<Response, ContractError> {
    match msg {
        ExecuteMsg::Transfer { recipient, amount } => {
            execute_transfer(deps, env, info, recipient, amount)
        }
        ExecuteMsg::Send {
            contract,
            amount,
            msg,
        } => execute_send(deps, env, info, contract, amount, msg),
        ExecuteMsg::IncreaseAllowance {
            spender,
            amount,
            expires,
        } => execute_increase_allowance(deps, env, info, spender, amount, expires),
        ExecuteMsg::DecreaseAllowance {
            spender,
            amount,
            expires,
        } => execute_decrease_allowance(deps, env, info, spender, amount, expires),
        ExecuteMsg::TransferFrom {
            owner,
            recipient,
            amount,
        } => execute_transfer_from(deps, env, info, owner, recipient, amount),
        ExecuteMsg::SendFrom {
            owner,
            contract,
            amount,
            msg,
        } => execute_send_from(deps, env, info, owner, contract, amount, msg),
        ExecuteMsg::UpdateMarketing {
            project,
            description,
            marketing,
        } => execute_update_marketing(deps, env, info, project, description, marketing),
    }
}

pub fn execute_transfer(
    deps: DepsMut,
    _env: Env,
    info: MessageInfo,
    recipient: String,
    amount: Uint128,
) -> Result<Response, ContractError> {
    if amount == Uint128::zero() {
        return Err(ContractError::InvalidZeroAmount {});
    }

    let rcpt_addr = deps.api.addr_validate(&recipient)?;

    BALANCES.update(
        deps.storage,
        &info.sender,
        |balance: Option<Uint128>| -> StdResult<_> {
            Ok(balance.unwrap_or_default().checked_sub(amount)?)
        },
    )?;
    BALANCES.update(
        deps.storage,
        &rcpt_addr,
        |balance: Option<Uint128>| -> StdResult<_> { Ok(balance.unwrap_or_default() + amount) },
    )?;

    let res = Response::new()
        .add_attribute("action", "transfer")
        .add_attribute("from", info.sender)
        .add_attribute("to", recipient)
        .add_attribute("amount", amount);
    Ok(res)
}

pub fn execute_send(
    deps: DepsMut,
    _env: Env,
    info: MessageInfo,
    contract: String,
    amount: Uint128,
    msg: Binary,
) -> Result<Response, ContractError> {
    if amount == Uint128::zero() {
        return Err(ContractError::InvalidZeroAmount {});
    }

    let rcpt_addr = deps.api.addr_validate(&contract)?;

    // move the tokens to the contract
    BALANCES.update(
        deps.storage,
        &info.sender,
        |balance: Option<Uint128>| -> StdResult<_> {
            Ok(balance.unwrap_or_default().checked_sub(amount)?)
        },
    )?;
    BALANCES.update(
        deps.storage,
        &rcpt_addr,
        |balance: Option<Uint128>| -> StdResult<_> { Ok(balance.unwrap_or_default() + amount) },
    )?;

    let res = Response::new()
        .add_attribute("action", "send")
        .add_attribute("from", &info.sender)
        .add_attribute("to", &contract)
        .add_attribute("amount", amount)
        .add_message(
            Cw20ReceiveMsg {
                sender: info.sender.into(),
                amount,
                msg,
            }
            .into_cosmos_msg(contract)?,
        );
    Ok(res)
}

pub fn execute_update_marketing(
    deps: DepsMut,
    _env: Env,
    info: MessageInfo,
    project: Option<String>,
    description: Option<String>,
    marketing: Option<String>,
) -> Result<Response, ContractError> {
    let mut marketing_info = MARKETING_INFO
        .may_load(deps.storage)?
        .ok_or(ContractError::Unauthorized {})?;

    if marketing_info
        .marketing
        .as_ref()
        .ok_or(ContractError::Unauthorized {})?
        != &info.sender
    {
        return Err(ContractError::Unauthorized {});
    }

    match project {
        Some(empty) if empty.trim().is_empty() => marketing_info.project = None,
        Some(project) => marketing_info.project = Some(project),
        None => (),
    }

    match description {
        Some(empty) if empty.trim().is_empty() => marketing_info.description = None,
        Some(description) => marketing_info.description = Some(description),
        None => (),
    }

    match marketing {
        Some(empty) if empty.trim().is_empty() => marketing_info.marketing = None,
        Some(marketing) => marketing_info.marketing = Some(deps.api.addr_validate(&marketing)?),
        None => (),
    }

    if marketing_info.project.is_none()
        && marketing_info.description.is_none()
        && marketing_info.marketing.is_none()
    {
        MARKETING_INFO.remove(deps.storage);
    } else {
        MARKETING_INFO.save(deps.storage, &marketing_info)?;
    }

    let res = Response::new().add_attribute("action", "update_marketing");
    Ok(res)
}

#[cfg_attr(not(feature = "library"), entry_point)]
pub fn query(deps: Deps, _env: Env, msg: QueryMsg) -> StdResult<Binary> {
    match msg {
        QueryMsg::Balance { address } => to_binary(&query_balance(deps, address)?),
        QueryMsg::TokenInfo {} => to_binary(&query_token_info(deps)?),
        QueryMsg::Allowance { owner, spender } => {
            to_binary(&query_allowance(deps, owner, spender)?)
        }
        QueryMsg::AllAllowances {
            owner,
            start_after,
            limit,
        } => to_binary(&query_all_allowances(deps, owner, start_after, limit)?),
        QueryMsg::AllAccounts { start_after, limit } => {
            to_binary(&query_all_accounts(deps, start_after, limit)?)
        }
        QueryMsg::MarketingInfo {} => to_binary(&query_marketing_info(deps)?),
    }
}

pub fn query_balance(deps: Deps, address: String) -> StdResult<BalanceResponse> {
    let address = deps.api.addr_validate(&address)?;
    let balance = BALANCES
        .may_load(deps.storage, &address)?
        .unwrap_or_default();
    Ok(BalanceResponse { balance })
}

pub fn query_token_info(deps: Deps) -> StdResult<TokenInfoResponse> {
    let info = TOKEN_INFO.load(deps.storage)?;
    let res = TokenInfoResponse {
        name: info.name,
        symbol: info.symbol,
        decimals: info.decimals,
        total_supply: info.total_supply,
    };
    Ok(res)
}

pub fn query_marketing_info(deps: Deps) -> StdResult<MarketingInfoResponse> {
    Ok(MARKETING_INFO.may_load(deps.storage)?.unwrap_or_default())
}

#[cfg(test)]
mod tests {
    use cosmwasm_std::testing::{
        mock_dependencies, mock_dependencies_with_balance, mock_env, mock_info,
    };
    use cosmwasm_std::{coins, from_binary, Addr, CosmosMsg, StdError, SubMsg, WasmMsg};

    use super::*;
    use crate::msg::InstantiateMarketingInfo;

    fn get_balance<T: Into<String>>(deps: Deps, address: T) -> Uint128 {
        query_balance(deps, address.into()).unwrap().balance
    }

    // this will set up the instantiation for other tests
    fn do_instantiate(deps: DepsMut, addr: &str, amount: Uint128) -> TokenInfoResponse {
        _do_instantiate(deps, addr, amount)
    }

    // this will set up the instantiation for other tests
    fn _do_instantiate(
        mut deps: DepsMut,
        addr: &str,
        amount: Uint128,
    ) -> TokenInfoResponse {
        let instantiate_msg = InstantiateMsg {
            name: "Auto Gen".to_string(),
            symbol: "AUTO".to_string(),
            decimals: 3,
            initial_balances: vec![Cw20Coin {
                address: addr.to_string(),
                amount,
            }],
            marketing: None,
        };
        let info = mock_info("creator", &[]);
        let env = mock_env();
        let res = instantiate(deps.branch(), env, info, instantiate_msg).unwrap();
        assert_eq!(0, res.messages.len());

        let meta = query_token_info(deps.as_ref()).unwrap();
        assert_eq!(
            meta,
            TokenInfoResponse {
                name: "Auto Gen".to_string(),
                symbol: "AUTO".to_string(),
                decimals: 3,
                total_supply: amount,
            }
        );
        assert_eq!(get_balance(deps.as_ref(), addr), amount);
        meta
    }

    const PNG_HEADER: [u8; 8] = [0x89, b'P', b'N', b'G', 0x0d, 0x0a, 0x1a, 0x0a];

    mod instantiate {
        use super::*;

        #[test]
        fn basic() {
            let mut deps = mock_dependencies();
            let amount = Uint128::from(11223344u128);
            let instantiate_msg = InstantiateMsg {
                name: "Cash Token".to_string(),
                symbol: "CASH".to_string(),
                decimals: 9,
                initial_balances: vec![Cw20Coin {
                    address: String::from("addr0000"),
                    amount,
                }],
                marketing: None,
            };
            let info = mock_info("creator", &[]);
            let env = mock_env();
            let res = instantiate(deps.as_mut(), env, info, instantiate_msg).unwrap();
            assert_eq!(0, res.messages.len());

            assert_eq!(
                query_token_info(deps.as_ref()).unwrap(),
                TokenInfoResponse {
                    name: "Cash Token".to_string(),
                    symbol: "CASH".to_string(),
                    decimals: 9,
                    total_supply: amount,
                }
            );
            assert_eq!(
                get_balance(deps.as_ref(), "addr0000"),
                Uint128::new(11223344)
            );
        }

        mod marketing {
            use super::*;

            #[test]
            fn basic() {
                let mut deps = mock_dependencies();
                let instantiate_msg = InstantiateMsg {
                    name: "Cash Token".to_string(),
                    symbol: "CASH".to_string(),
                    decimals: 9,
                    initial_balances: vec![],
                    marketing: Some(InstantiateMarketingInfo {
                        project: Some("Project".to_owned()),
                        description: Some("Description".to_owned()),
                        marketing: Some("marketing".to_owned()),
                    }),
                };

                let info = mock_info("creator", &[]);
                let env = mock_env();
                let res = instantiate(deps.as_mut(), env, info, instantiate_msg).unwrap();
                assert_eq!(0, res.messages.len());

                assert_eq!(
                    query_marketing_info(deps.as_ref()).unwrap(),
                    MarketingInfoResponse {
                        project: Some("Project".to_owned()),
                        description: Some("Description".to_owned()),
                        marketing: Some(Addr::unchecked("marketing")),
                    }
                );
            }

            #[test]
            fn invalid_marketing() {
                let mut deps = mock_dependencies();
                let instantiate_msg = InstantiateMsg {
                    name: "Cash Token".to_string(),
                    symbol: "CASH".to_string(),
                    decimals: 9,
                    initial_balances: vec![],
                    marketing: Some(InstantiateMarketingInfo {
                        project: Some("Project".to_owned()),
                        description: Some("Description".to_owned()),
                        marketing: Some("m".to_owned()),
                    }),
                };

                let info = mock_info("creator", &[]);
                let env = mock_env();
                instantiate(deps.as_mut(), env, info, instantiate_msg).unwrap_err();
            }
        }
    }

    #[test]
    fn instantiate_multiple_accounts() {
        let mut deps = mock_dependencies();
        let amount1 = Uint128::from(11223344u128);
        let addr1 = String::from("addr0001");
        let amount2 = Uint128::from(7890987u128);
        let addr2 = String::from("addr0002");
        let instantiate_msg = InstantiateMsg {
            name: "Bash Shell".to_string(),
            symbol: "BASH".to_string(),
            decimals: 6,
            initial_balances: vec![
                Cw20Coin {
                    address: addr1.clone(),
                    amount: amount1,
                },
                Cw20Coin {
                    address: addr2.clone(),
                    amount: amount2,
                },
            ],
            marketing: None,
        };
        let info = mock_info("creator", &[]);
        let env = mock_env();
        let res = instantiate(deps.as_mut(), env, info, instantiate_msg).unwrap();
        assert_eq!(0, res.messages.len());

        assert_eq!(
            query_token_info(deps.as_ref()).unwrap(),
            TokenInfoResponse {
                name: "Bash Shell".to_string(),
                symbol: "BASH".to_string(),
                decimals: 6,
                total_supply: amount1 + amount2,
            }
        );
        assert_eq!(get_balance(deps.as_ref(), addr1), amount1);
        assert_eq!(get_balance(deps.as_ref(), addr2), amount2);
    }

    #[test]
    fn queries_work() {
        let mut deps = mock_dependencies_with_balance(&coins(2, "token"));
        let addr1 = String::from("addr0001");
        let amount1 = Uint128::from(12340000u128);

        let expected = do_instantiate(deps.as_mut(), &addr1, amount1);

        // check meta query
        let loaded = query_token_info(deps.as_ref()).unwrap();
        assert_eq!(expected, loaded);

        let _info = mock_info("test", &[]);
        let env = mock_env();
        // check balance query (full)
        let data = query(
            deps.as_ref(),
            env.clone(),
            QueryMsg::Balance { address: addr1 },
        )
        .unwrap();
        let loaded: BalanceResponse = from_binary(&data).unwrap();
        assert_eq!(loaded.balance, amount1);

        // check balance query (empty)
        let data = query(
            deps.as_ref(),
            env,
            QueryMsg::Balance {
                address: String::from("addr0002"),
            },
        )
        .unwrap();
        let loaded: BalanceResponse = from_binary(&data).unwrap();
        assert_eq!(loaded.balance, Uint128::zero());
    }

    #[test]
    fn transfer() {
        let mut deps = mock_dependencies_with_balance(&coins(2, "token"));
        let addr1 = String::from("addr0001");
        let addr2 = String::from("addr0002");
        let amount1 = Uint128::from(12340000u128);
        let transfer = Uint128::from(76543u128);
        let too_much = Uint128::from(12340321u128);

        do_instantiate(deps.as_mut(), &addr1, amount1);

        // cannot transfer nothing
        let info = mock_info(addr1.as_ref(), &[]);
        let env = mock_env();
        let msg = ExecuteMsg::Transfer {
            recipient: addr2.clone(),
            amount: Uint128::zero(),
        };
        let err = execute(deps.as_mut(), env, info, msg).unwrap_err();
        assert_eq!(err, ContractError::InvalidZeroAmount {});

        // cannot send more than we have
        let info = mock_info(addr1.as_ref(), &[]);
        let env = mock_env();
        let msg = ExecuteMsg::Transfer {
            recipient: addr2.clone(),
            amount: too_much,
        };
        let err = execute(deps.as_mut(), env, info, msg).unwrap_err();
        assert!(matches!(err, ContractError::Std(StdError::Overflow { .. })));

        // cannot send from empty account
        let info = mock_info(addr2.as_ref(), &[]);
        let env = mock_env();
        let msg = ExecuteMsg::Transfer {
            recipient: addr1.clone(),
            amount: transfer,
        };
        let err = execute(deps.as_mut(), env, info, msg).unwrap_err();
        assert!(matches!(err, ContractError::Std(StdError::Overflow { .. })));

        // valid transfer
        let info = mock_info(addr1.as_ref(), &[]);
        let env = mock_env();
        let msg = ExecuteMsg::Transfer {
            recipient: addr2.clone(),
            amount: transfer,
        };
        let res = execute(deps.as_mut(), env, info, msg).unwrap();
        assert_eq!(res.messages.len(), 0);

        let remainder = amount1.checked_sub(transfer).unwrap();
        assert_eq!(get_balance(deps.as_ref(), addr1), remainder);
        assert_eq!(get_balance(deps.as_ref(), addr2), transfer);
        assert_eq!(
            query_token_info(deps.as_ref()).unwrap().total_supply,
            amount1
        );
    }

    #[test]
    fn send() {
        let mut deps = mock_dependencies_with_balance(&coins(2, "token"));
        let addr1 = String::from("addr0001");
        let contract = String::from("addr0002");
        let amount1 = Uint128::from(12340000u128);
        let transfer = Uint128::from(76543u128);
        let too_much = Uint128::from(12340321u128);
        let send_msg = Binary::from(r#"{"some":123}"#.as_bytes());

        do_instantiate(deps.as_mut(), &addr1, amount1);

        // cannot send nothing
        let info = mock_info(addr1.as_ref(), &[]);
        let env = mock_env();
        let msg = ExecuteMsg::Send {
            contract: contract.clone(),
            amount: Uint128::zero(),
            msg: send_msg.clone(),
        };
        let err = execute(deps.as_mut(), env, info, msg).unwrap_err();
        assert_eq!(err, ContractError::InvalidZeroAmount {});

        // cannot send more than we have
        let info = mock_info(addr1.as_ref(), &[]);
        let env = mock_env();
        let msg = ExecuteMsg::Send {
            contract: contract.clone(),
            amount: too_much,
            msg: send_msg.clone(),
        };
        let err = execute(deps.as_mut(), env, info, msg).unwrap_err();
        assert!(matches!(err, ContractError::Std(StdError::Overflow { .. })));

        // valid transfer
        let info = mock_info(addr1.as_ref(), &[]);
        let env = mock_env();
        let msg = ExecuteMsg::Send {
            contract: contract.clone(),
            amount: transfer,
            msg: send_msg.clone(),
        };
        let res = execute(deps.as_mut(), env, info, msg).unwrap();
        assert_eq!(res.messages.len(), 1);

        // ensure proper send message sent
        // this is the message we want delivered to the other side
        let binary_msg = Cw20ReceiveMsg {
            sender: addr1.clone(),
            amount: transfer,
            msg: send_msg,
        }
        .into_binary()
        .unwrap();
        // and this is how it must be wrapped for the vm to process it
        assert_eq!(
            res.messages[0],
            SubMsg::new(CosmosMsg::Wasm(WasmMsg::Execute {
                contract_addr: contract.clone(),
                msg: binary_msg,
                funds: vec![],
            }))
        );

        // ensure balance is properly transferred
        let remainder = amount1.checked_sub(transfer).unwrap();
        assert_eq!(get_balance(deps.as_ref(), addr1), remainder);
        assert_eq!(get_balance(deps.as_ref(), contract), transfer);
        assert_eq!(
            query_token_info(deps.as_ref()).unwrap().total_supply,
            amount1
        );
    }

    mod marketing {
        use super::*;

        #[test]
        fn update_unauthorised() {
            let mut deps = mock_dependencies();
            let instantiate_msg = InstantiateMsg {
                name: "Cash Token".to_string(),
                symbol: "CASH".to_string(),
                decimals: 9,
                initial_balances: vec![],
                marketing: Some(InstantiateMarketingInfo {
                    project: Some("Project".to_owned()),
                    description: Some("Description".to_owned()),
                    marketing: Some("marketing".to_owned()),
                }),
            };

            let info = mock_info("creator", &[]);

            instantiate(deps.as_mut(), mock_env(), info.clone(), instantiate_msg).unwrap();

            let err = execute(
                deps.as_mut(),
                mock_env(),
                info,
                ExecuteMsg::UpdateMarketing {
                    project: Some("New project".to_owned()),
                    description: Some("Better description".to_owned()),
                    marketing: Some("creator".to_owned()),
                },
            )
            .unwrap_err();

            assert_eq!(err, ContractError::Unauthorized {});

            // Ensure marketing didn't change
            assert_eq!(
                query_marketing_info(deps.as_ref()).unwrap(),
                MarketingInfoResponse {
                    project: Some("Project".to_owned()),
                    description: Some("Description".to_owned()),
                    marketing: Some(Addr::unchecked("marketing")),
                }
            );
        }

        #[test]
        fn update_project() {
            let mut deps = mock_dependencies();
            let instantiate_msg = InstantiateMsg {
                name: "Cash Token".to_string(),
                symbol: "CASH".to_string(),
                decimals: 9,
                initial_balances: vec![],
                marketing: Some(InstantiateMarketingInfo {
                    project: Some("Project".to_owned()),
                    description: Some("Description".to_owned()),
                    marketing: Some("creator".to_owned()),
                }),
            };

            let info = mock_info("creator", &[]);

            instantiate(deps.as_mut(), mock_env(), info.clone(), instantiate_msg).unwrap();

            let res = execute(
                deps.as_mut(),
                mock_env(),
                info,
                ExecuteMsg::UpdateMarketing {
                    project: Some("New project".to_owned()),
                    description: None,
                    marketing: None,
                },
            )
            .unwrap();

            assert_eq!(res.messages, vec![]);

            assert_eq!(
                query_marketing_info(deps.as_ref()).unwrap(),
                MarketingInfoResponse {
                    project: Some("New project".to_owned()),
                    description: Some("Description".to_owned()),
                    marketing: Some(Addr::unchecked("creator")),
                }
            );
        }

        #[test]
        fn clear_project() {
            let mut deps = mock_dependencies();
            let instantiate_msg = InstantiateMsg {
                name: "Cash Token".to_string(),
                symbol: "CASH".to_string(),
                decimals: 9,
                initial_balances: vec![],
                marketing: Some(InstantiateMarketingInfo {
                    project: Some("Project".to_owned()),
                    description: Some("Description".to_owned()),
                    marketing: Some("creator".to_owned()),
                }),
            };

            let info = mock_info("creator", &[]);

            instantiate(deps.as_mut(), mock_env(), info.clone(), instantiate_msg).unwrap();

            let res = execute(
                deps.as_mut(),
                mock_env(),
                info,
                ExecuteMsg::UpdateMarketing {
                    project: Some("".to_owned()),
                    description: None,
                    marketing: None,
                },
            )
            .unwrap();

            assert_eq!(res.messages, vec![]);

            assert_eq!(
                query_marketing_info(deps.as_ref()).unwrap(),
                MarketingInfoResponse {
                    project: None,
                    description: Some("Description".to_owned()),
                    marketing: Some(Addr::unchecked("creator")),
                }
            );
        }

        #[test]
        fn update_description() {
            let mut deps = mock_dependencies();
            let instantiate_msg = InstantiateMsg {
                name: "Cash Token".to_string(),
                symbol: "CASH".to_string(),
                decimals: 9,
                initial_balances: vec![],
                marketing: Some(InstantiateMarketingInfo {
                    project: Some("Project".to_owned()),
                    description: Some("Description".to_owned()),
                    marketing: Some("creator".to_owned()),
                }),
            };

            let info = mock_info("creator", &[]);

            instantiate(deps.as_mut(), mock_env(), info.clone(), instantiate_msg).unwrap();

            let res = execute(
                deps.as_mut(),
                mock_env(),
                info,
                ExecuteMsg::UpdateMarketing {
                    project: None,
                    description: Some("Better description".to_owned()),
                    marketing: None,
                },
            )
            .unwrap();

            assert_eq!(res.messages, vec![]);

            assert_eq!(
                query_marketing_info(deps.as_ref()).unwrap(),
                MarketingInfoResponse {
                    project: Some("Project".to_owned()),
                    description: Some("Better description".to_owned()),
                    marketing: Some(Addr::unchecked("creator")),
                }
            );
        }

        #[test]
        fn clear_description() {
            let mut deps = mock_dependencies();
            let instantiate_msg = InstantiateMsg {
                name: "Cash Token".to_string(),
                symbol: "CASH".to_string(),
                decimals: 9,
                initial_balances: vec![],
                marketing: Some(InstantiateMarketingInfo {
                    project: Some("Project".to_owned()),
                    description: Some("Description".to_owned()),
                    marketing: Some("creator".to_owned()),
                }),
            };

            let info = mock_info("creator", &[]);

            instantiate(deps.as_mut(), mock_env(), info.clone(), instantiate_msg).unwrap();

            let res = execute(
                deps.as_mut(),
                mock_env(),
                info,
                ExecuteMsg::UpdateMarketing {
                    project: None,
                    description: Some("".to_owned()),
                    marketing: None,
                },
            )
            .unwrap();

            assert_eq!(res.messages, vec![]);

            assert_eq!(
                query_marketing_info(deps.as_ref()).unwrap(),
                MarketingInfoResponse {
                    project: Some("Project".to_owned()),
                    description: None,
                    marketing: Some(Addr::unchecked("creator")),
                }
            );
        }

        #[test]
        fn update_marketing() {
            let mut deps = mock_dependencies();
            let instantiate_msg = InstantiateMsg {
                name: "Cash Token".to_string(),
                symbol: "CASH".to_string(),
                decimals: 9,
                initial_balances: vec![],
                marketing: Some(InstantiateMarketingInfo {
                    project: Some("Project".to_owned()),
                    description: Some("Description".to_owned()),
                    marketing: Some("creator".to_owned()),
                }),
            };

            let info = mock_info("creator", &[]);

            instantiate(deps.as_mut(), mock_env(), info.clone(), instantiate_msg).unwrap();

            let res = execute(
                deps.as_mut(),
                mock_env(),
                info,
                ExecuteMsg::UpdateMarketing {
                    project: None,
                    description: None,
                    marketing: Some("marketing".to_owned()),
                },
            )
            .unwrap();

            assert_eq!(res.messages, vec![]);

            assert_eq!(
                query_marketing_info(deps.as_ref()).unwrap(),
                MarketingInfoResponse {
                    project: Some("Project".to_owned()),
                    description: Some("Description".to_owned()),
                    marketing: Some(Addr::unchecked("marketing")),
                }
            );
        }

        #[test]
        fn update_marketing_invalid() {
            let mut deps = mock_dependencies();
            let instantiate_msg = InstantiateMsg {
                name: "Cash Token".to_string(),
                symbol: "CASH".to_string(),
                decimals: 9,
                initial_balances: vec![],
                marketing: Some(InstantiateMarketingInfo {
                    project: Some("Project".to_owned()),
                    description: Some("Description".to_owned()),
                    marketing: Some("creator".to_owned()),
                }),
            };

            let info = mock_info("creator", &[]);

            instantiate(deps.as_mut(), mock_env(), info.clone(), instantiate_msg).unwrap();

            let err = execute(
                deps.as_mut(),
                mock_env(),
                info,
                ExecuteMsg::UpdateMarketing {
                    project: None,
                    description: None,
                    marketing: Some("m".to_owned()),
                },
            )
            .unwrap_err();

            assert!(
                matches!(err, ContractError::Std(_)),
                "Expected Std error, received: {}",
                err
            );

            assert_eq!(
                query_marketing_info(deps.as_ref()).unwrap(),
                MarketingInfoResponse {
                    project: Some("Project".to_owned()),
                    description: Some("Description".to_owned()),
                    marketing: Some(Addr::unchecked("creator")),
                }
            );
        }

        #[test]
        fn clear_marketing() {
            let mut deps = mock_dependencies();
            let instantiate_msg = InstantiateMsg {
                name: "Cash Token".to_string(),
                symbol: "CASH".to_string(),
                decimals: 9,
                initial_balances: vec![],
                marketing: Some(InstantiateMarketingInfo {
                    project: Some("Project".to_owned()),
                    description: Some("Description".to_owned()),
                    marketing: Some("creator".to_owned()),
                }),
            };

            let info = mock_info("creator", &[]);

            instantiate(deps.as_mut(), mock_env(), info.clone(), instantiate_msg).unwrap();

            let res = execute(
                deps.as_mut(),
                mock_env(),
                info,
                ExecuteMsg::UpdateMarketing {
                    project: None,
                    description: None,
                    marketing: Some("".to_owned()),
                },
            )
            .unwrap();

            assert_eq!(res.messages, vec![]);

            assert_eq!(
                query_marketing_info(deps.as_ref()).unwrap(),
                MarketingInfoResponse {
                    project: Some("Project".to_owned()),
                    description: Some("Description".to_owned()),
                    marketing: None,
                }
            );
        }
    }
}
