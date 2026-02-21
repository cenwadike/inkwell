//!
//! Contract implements a fixed-cost token sales contract.
//!
//! - User can create a new market.
//! - Market stores the base token, quote token, and exchange rate.
//! - User can swap base token for quote token.
//! - User can swap quote token for base token.
//!
//! The program is ABI-equivalent with Solidity, meaning you can call it from Solidity and Rust.
//! To do this, run `cargo stylus export-abi`.
//!
//! Note: this code is a technical demonstration and has not been audited.
//!

// Allow `cargo stylus export-abi` to generate a main function.
#![cfg_attr(not(feature = "export-abi"), no_main)]
extern crate alloc;

use alloy_sol_types::sol;
#[allow(deprecated)]
use stylus_sdk::{
    alloy_primitives::{Address, U256, U64},
    call::Call,
    prelude::*,
};

// Define some persistent storage using the Solidity ABI.
// `Contract` will be the entry point.
sol_storage! {
    #[entrypoint]
    pub struct Contract {
        // Initialization status
        bool initialized;
        // Market Index
        uint64 market_index;
        // Maps market index to Market data.
        mapping(uint64 => Market) markets;
        // Maps base token and quote token address to a market index.
        mapping(address => mapping(address => uint64)) indexes;
    }

    // Market consists of a base token, quote token, and market rate
    pub struct Market {
        address base_token;     // eg. ETH in ETH/USDT
        address quote_token;    // eg. USDT in ETH/USDT
        uint256 exchange_rate;  // eg. ETH/USDT exchange is 3500
    }
}

// Define Erc20 interface
sol_interface! {
    interface IErc20 {
        function transfer(address,uint256) external;
        function transferFrom(address,address,uint256) external returns (bool);
    }
}

// Define events and error types
sol! {
    // Events for the Contract
    event Initialized();
    event MarketCreated(address indexed base_token, address indexed quote_token, uint256 exchange_rate);
    event SwappedBaseTokenForQuoteToken(address indexed base_token, address indexed quote_token, uint256 amount_in, uint256 amount_out);

    // Error types for the Contract
    error AlreadyInitialized();
    error MarketExists();
    error BaseTokenCanNotBeZeroAddress();
    error QuoteTokenCanNotBeZeroAddress();
    error ExchangeRateCanNotBeZero();
    error AmountCanNotBeZero();
    error IncorrectBaseAmount();
    error IncorrectQuoteAmount();
    error DivisionUnderflow();
    error MultiplicationOverflow();
    error OutOfBoundIndex();
}

/// Represents the ways methods may fail.
#[derive(SolidityError)]
pub enum ContractError {
    AlreadyInitialized(AlreadyInitialized),
    MarketExists(MarketExists),
    BaseTokenCanNotBeZeroAddress(BaseTokenCanNotBeZeroAddress),
    QuoteTokenCanNotBeZeroAddress(QuoteTokenCanNotBeZeroAddress),
    ExchangeRateCanNotBeZero(ExchangeRateCanNotBeZero),
    AmountCanNotBeZero(AmountCanNotBeZero),
    IncorrectBaseAmount(IncorrectBaseAmount),
    IncorrectQuoteAmount(IncorrectQuoteAmount),
    DivisionUnderflow(DivisionUnderflow),
    MultiplicationOverflow(MultiplicationOverflow),
    OutOfBoundIndex(OutOfBoundIndex),
}

/// Declare that `Contract` is a contract with the following external methods.
#[public]
impl Contract {
    /// Initialize contract.
    pub fn initialize(&mut self) -> Result<(), ContractError> {
        // Ensure the contract has not been initialized.
        if self.initialized.get() {
            return Err(ContractError::AlreadyInitialized(AlreadyInitialized {}));
        }

        // Set initialized.
        self.initialized.set(true);

        // Initialize market index.
        self.market_index.set(U64::from(1));

        // Emit event
        stylus_sdk::stylus_core::log(self.vm(), Initialized {});

        Ok(())
    }

    /// Create a new market.
    ///
    /// Return market index.
    #[allow(deprecated)]
    pub fn create_market(
        &mut self,
        base_token: Address,
        quote_token: Address,
        exchange_rate: U256, // eg. 3.
        base_amount: U256,   // eg. 2.
        quote_amount: U256,  // eg. base_amount * rate; 2 * 3 = 6.
    ) -> Result<U256, ContractError> {
        // Ensures rate is not 0.
        if exchange_rate == U256::from(0) {
            return Err(ContractError::ExchangeRateCanNotBeZero(
                ExchangeRateCanNotBeZero {},
            ));
        }

        // Ensures the base token address is not a zero address.
        if base_token == Address::ZERO {
            return Err(ContractError::BaseTokenCanNotBeZeroAddress(
                BaseTokenCanNotBeZeroAddress {},
            ));
        }

        // Ensures the quote token address is not a zero address.
        if quote_token == Address::ZERO {
            return Err(ContractError::QuoteTokenCanNotBeZeroAddress(
                QuoteTokenCanNotBeZeroAddress {},
            ));
        }

        // Safely unwrap the calculated base token amount.
        let expected_base_amount = quote_amount.checked_div(exchange_rate);
        if expected_base_amount.is_none() {
            return Err(ContractError::DivisionUnderflow(DivisionUnderflow {}));
        }

        // Safely unwrap the calculated base token amount.
        let expected_base_amount = expected_base_amount.unwrap();

        // Ensure the correct amount of base token was supplied.
        if base_amount.ne(&expected_base_amount) {
            return Err(ContractError::IncorrectBaseAmount(IncorrectBaseAmount {}));
        }

        // Calculate the expected quote token amount.
        let expected_quote_amount = base_amount.checked_mul(exchange_rate);
        if expected_quote_amount.is_none() {
            return Err(ContractError::MultiplicationOverflow(
                MultiplicationOverflow {},
            ));
        }

        // Safely unwrap the calculated quote token amount.
        let expected_quote_amount = expected_quote_amount.unwrap();

        // Assert enough quote token was supplied.
        if quote_amount.ne(&expected_quote_amount) {
            return Err(ContractError::IncorrectQuoteAmount(IncorrectQuoteAmount {}));
        }

        // Get the current market index.
        let mut current_market_index = self.market_index.get();

        // Ensure the market does not exist.
        let mut base_token_map = self.indexes.setter(base_token);
        let quote_token_map = base_token_map.setter(quote_token);
        let market_index = quote_token_map.get();

        // Return error if the market exists.
        if !market_index.is_zero() {
            return Err(ContractError::MarketExists(MarketExists {}));
        }

        // Create a new market in storage.
        let mut market = self.markets.setter(current_market_index);
        market.base_token.set(base_token);
        market.quote_token.set(quote_token);
        market.exchange_rate.set(exchange_rate);

        // Map (base_token_address, quote_token_address) => market_index.
        let mut base_token_map = self.indexes.setter(base_token);
        let mut quote_token_map = base_token_map.setter(quote_token);
        quote_token_map.set(current_market_index);

        // Set new market index.
        current_market_index += U64::from(1);
        self.market_index.set(current_market_index);

        let sender = self.vm().msg_sender();
        let contract = self.vm().contract_address();

        // Transfer base token from creator.
        let base_token_contract = IErc20::new(base_token);
        let _ = base_token_contract.transfer_from(Call::new(), sender, contract, base_amount);

        // Transfer quote token from creator.
        let quote_token_contract = IErc20::new(quote_token);
        let _ = quote_token_contract.transfer_from(Call::new(), sender, contract, quote_amount);

        // Emit event.
        stylus_sdk::stylus_core::log(
            self.vm(),
            MarketCreated {
                base_token,
                quote_token,
                exchange_rate,
            },
        );

        // Return market index.
        Ok(U256::from(current_market_index))
    }

    /// Swap base token for quote token.
    #[allow(deprecated)]
    pub fn swap_base_token_for_quote_token(
        &mut self,
        base_token: Address,
        quote_token: Address,
        base_amount: U256,
    ) -> Result<(), ContractError> {
        // Ensures base amount is not 0.
        if base_amount == U256::from(0) {
            return Err(ContractError::AmountCanNotBeZero(AmountCanNotBeZero {}));
        }

        // Ensures base token address is not a zero address.
        if base_token == Address::ZERO {
            return Err(ContractError::BaseTokenCanNotBeZeroAddress(
                BaseTokenCanNotBeZeroAddress {},
            ));
        }

        // Ensures quote token address is not a zero address.
        if quote_token == Address::ZERO {
            return Err(ContractError::QuoteTokenCanNotBeZeroAddress(
                QuoteTokenCanNotBeZeroAddress {},
            ));
        }

        // Get market from the base token and quote token.
        let mut base_token_map = self.indexes.setter(base_token);
        let quote_token_map = base_token_map.setter(quote_token);
        let market_index = quote_token_map.get();

        // Get market.
        let market = self.markets.get(market_index);

        // Get market rate.
        let exchange_rate = market.exchange_rate.get();

        // Calculate the quote token amount.
        let quote_amount = base_amount.checked_mul(exchange_rate);

        // Return overflow error.
        if quote_amount.is_none() {
            return Err(ContractError::MultiplicationOverflow(
                MultiplicationOverflow {},
            ));
        }

        // Safely unwrap the quote amount.
        let quote_amount = quote_amount.unwrap();

        let sender = self.vm().msg_sender();
        let contract = self.vm().contract_address();

        // Transfer base token from user.
        let base_token_contract = IErc20::new(market.base_token.get());
        let _ = base_token_contract.transfer_from(Call::new(), sender, contract, base_amount);

        // Transfer quote token to user.
        let quote_token_contract = IErc20::new(market.quote_token.get());
        let _ = quote_token_contract.transfer(Call::new(), sender, quote_amount);

        // Emit event.
        stylus_sdk::stylus_core::log(
            self.vm(),
            SwappedBaseTokenForQuoteToken {
                base_token,
                quote_token,
                amount_in: base_amount,
                amount_out: quote_amount,
            },
        );

        Ok(())
    }

    /// Swap quote token for base token.
    #[allow(deprecated)]
    pub fn swap_quote_token_for_base_token(
        &mut self,
        base_token: Address,
        quote_token: Address,
        quote_amount: U256,
    ) -> Result<(), ContractError> {
        // Ensures amount is not 0.
        if quote_amount == U256::from(0) {
            return Err(ContractError::AmountCanNotBeZero(AmountCanNotBeZero {}));
        }

        // Ensures the base token address is not a zero address.
        if base_token == Address::ZERO {
            return Err(ContractError::BaseTokenCanNotBeZeroAddress(
                BaseTokenCanNotBeZeroAddress {},
            ));
        }

        // Ensures the quote token address is not a zero address.
        if quote_token == Address::ZERO {
            return Err(ContractError::QuoteTokenCanNotBeZeroAddress(
                QuoteTokenCanNotBeZeroAddress {},
            ));
        }

        // Get market from the base token and quote token.
        let mut base_token_map = self.indexes.setter(base_token);
        let quote_token_map = base_token_map.setter(quote_token);
        let market_index = quote_token_map.get();

        // Get market.
        let market = self.markets.get(market_index);

        // Get market rate.
        let exchange_rate = market.exchange_rate.get();

        // Calculate the base token amount.
        let base_amount = quote_amount.checked_div(exchange_rate);

        // Return underflow error.
        if base_amount.is_none() {
            return Err(ContractError::DivisionUnderflow(DivisionUnderflow {}));
        }

        // Safely unwrap the base amount.
        let base_amount = base_amount.unwrap();

        let sender = self.vm().msg_sender();
        let contract = self.vm().contract_address();

        // Transfer quote token to contract.
        let quote_token_contract = IErc20::new(market.quote_token.get());
        let _ = quote_token_contract.transfer_from(Call::new(), sender, contract, quote_amount);

        // Transfer base token to user.
        let base_token_contract = IErc20::new(market.base_token.get());
        let _ = base_token_contract.transfer(Call::new(), sender, base_amount);

        Ok(())
    }

    /// Fetch initialization status.
    pub fn fetch_initialization_status(&self) -> Result<bool, ContractError> {
        Ok(self.initialized.get())
    }

    /// Fetch current market index.
    pub fn fetch_current_market_index(&self) -> Result<U256, ContractError> {
        Ok(U256::from(self.market_index.get()))
    }

    /// Fetch exchange rate.
    pub fn fetch_exchange_rate(
        &self,
        base_token: Address,
        quote_token: Address,
    ) -> Result<U256, ContractError> {
        // Ensures base token address is not a zero address.
        if base_token == Address::ZERO {
            return Err(ContractError::BaseTokenCanNotBeZeroAddress(
                BaseTokenCanNotBeZeroAddress {},
            ));
        }

        // Ensures quote token address is not a zero address.
        if quote_token == Address::ZERO {
            return Err(ContractError::QuoteTokenCanNotBeZeroAddress(
                QuoteTokenCanNotBeZeroAddress {},
            ));
        }

        // Get market from the base token and quote token.
        let base_token_map = self.indexes.getter(base_token);
        let quote_token_map = base_token_map.getter(quote_token);
        let market_index = quote_token_map.get();

        // Get market.
        let market = self.markets.get(market_index);

        // Get the exchange rate.
        let exchange_rate = market.exchange_rate.get();

        Ok(exchange_rate)
    }

    /// Fetch market index.
    pub fn fetch_market_id(
        &self,
        base_token: Address,
        quote_token: Address,
    ) -> Result<U256, ContractError> {
        // Ensures the base token address is not a zero address.
        if base_token == Address::ZERO {
            return Err(ContractError::BaseTokenCanNotBeZeroAddress(
                BaseTokenCanNotBeZeroAddress {},
            ));
        }

        // Ensures the quote token address is not a zero address.
        if quote_token == Address::ZERO {
            return Err(ContractError::QuoteTokenCanNotBeZeroAddress(
                QuoteTokenCanNotBeZeroAddress {},
            ));
        }

        // Get market from the base token and quote token.
        let base_token_map = self.indexes.getter(base_token);
        let quote_token_map = base_token_map.getter(quote_token);
        let market_index = quote_token_map.get();

        Ok(U256::from(market_index))
    }

    /// Fetch market by tokens.
    pub fn fetch_market_by_tokens(
        &self,
        base_token: Address,
        quote_token: Address,
    ) -> Result<(Address, Address, U256), ContractError> {
        // Ensures the base token address is not a zero address.
        if base_token == Address::ZERO {
            return Err(ContractError::BaseTokenCanNotBeZeroAddress(
                BaseTokenCanNotBeZeroAddress {},
            ));
        }

        // Ensures the quote token address is not a zero address.
        if quote_token == Address::ZERO {
            return Err(ContractError::QuoteTokenCanNotBeZeroAddress(
                QuoteTokenCanNotBeZeroAddress {},
            ));
        }

        // Get market from the base token and quote token.
        let base_token_map = self.indexes.getter(base_token);
        let quote_token_map = base_token_map.getter(quote_token);
        let market_index = quote_token_map.get();

        // Get market.
        let market = self.markets.get(U64::from(market_index));

        Ok((
            market.base_token.get(),
            market.quote_token.get(),
            market.exchange_rate.get(),
        ))
    }

    /// Fetch market by id.
    /// Useful for pagination.
    ///
    /// Return market (base_token, quote_token, exchange_rate).
    pub fn fetch_market_by_id(
        &self,
        market_index: u64,
    ) -> Result<(Address, Address, U256), ContractError> {
        // Ensure the index is valid.
        if U64::from(market_index).ge(&self.market_index.get())
            || U64::from(market_index).eq(&U64::from(0))
        {
            return Err(ContractError::OutOfBoundIndex(OutOfBoundIndex {}));
        }

        // Get market.
        let market = self.markets.get(U64::from(market_index));

        Ok((
            market.base_token.get(),
            market.quote_token.get(),
            market.exchange_rate.get(),
        ))
    }
}
