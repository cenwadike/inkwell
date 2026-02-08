use stylus_sdk::{evm, msg, prelude::*, storage::StorageMap};

sol_storage! {
    pub struct Token {
        StorageMap<Address, U256> balances;
        StorageMap<Address, StorageMap<Address, U256>> allowances;
        U256 total_supply;
    }
}

#[external]
impl Token {
    pub fn transfer(&mut self, to: Address, amount: U256) -> Result<(), Vec<u8>> {
        let sender = msg::sender();

        let balance = self.balances.get(sender);
        require!(balance >= amount, "Insufficient balance");

        self.balances.insert(sender, balance - amount);
        self.balances.insert(to, self.balances.get(to) + amount);

        evm::log(Transfer {
            from: sender,
            to,
            amount,
        });

        Ok(())
    }

    pub fn approve(&mut self, spender: Address, amount: U256) -> Result<(), Vec<u8>> {
        let owner = msg::sender();

        self.allowances.get(owner).insert(spender, amount);

        evm::log(Approval {
            owner,
            spender,
            amount,
        });

        Ok(())
    }

    pub fn transfer_from(
        &mut self,
        from: Address,
        to: Address,
        amount: U256,
    ) -> Result<(), Vec<u8>> {
        let spender = msg::sender();

        let allowance = self.allowances.get(from).get(spender);
        require!(allowance >= amount, "Insufficient allowance");

        let balance = self.balances.get(from);
        require!(balance >= amount, "Insufficient balance");

        self.allowances
            .get(from)
            .insert(spender, allowance - amount);
        self.balances.insert(from, balance - amount);
        self.balances.insert(to, self.balances.get(to) + amount);

        evm::log(Transfer { from, to, amount });

        Ok(())
    }
}

sol! {
    event Transfer(address indexed from, address indexed to, uint256 amount);
    event Approval(address indexed owner, address indexed spender, uint256 amount);
}
