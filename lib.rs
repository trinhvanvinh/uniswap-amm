#![cfg_attr(not(feature = "std"), no_std)]
#![allow(non_snake_case)]

const PRECISION: u128 = 1_000_000;

#[ink::contract]
mod amm {
    use ink::storage::Mapping;

    /// Defines the storage of your contract.
    /// Add new fields to the below struct in order
    /// to add new static storage fields to your contract.
    #[derive(Default)]
    #[ink(storage)]
    pub struct Amm {
        /// Stores a single `bool` value on the storage.
        totalShares: Balance,
        totalToken1: Balance,
        totalToken2: Balance,
        shares: Mapping<AccountId, Balance>,
        token1Balance: Mapping<AccountId, Balance>,
        token2Balance: Mapping<AccountId, Balance>,
        fees: Balance,
    }

    pub enum CheckTypes {
        shares,
        token1Balance,
        token2Balance,
    }

    #[derive(Debug, PartialEq, Eq, scale::Encode, scale::Decode)]
    #[cfg_attr(feature = "std", derive(scale_info::TypeInfo))]
    pub enum Error {
        ZeroLiquidity,
        ZeroAmount,
        InsufficientAmount,
        NonEquivalentValue,
        ThresholdNotReached,
        InvalidShare,
        InsufficientLiquidity,
        SlippageExceeded,
    }

    impl Amm {
        /// Constructor that initializes the `bool` value to the given `init_value`.
        #[ink(constructor)]
        pub fn new(_fees: Balance) -> Self {
            Self {
                fees: if _fees >= 1000 { 0 } else { _fees },
                ..Default::default()
            }
        }

        #[ink(message)]
        pub fn faucet(&mut self, _amountToken1: Balance, _amountToken2: Balance) {
            let caller = self.env().caller();
            let token1 = self.token1Balance.get(caller).unwrap_or(0);
            let token2 = self.token2Balance.get(caller).unwrap_or(0);

            self.token1Balance.insert(caller, &(token1 + _amountToken1));
            self.token2Balance.insert(caller, &(token2 + _amountToken2));
        }

        #[ink(message)]
        pub fn getMyHoldings(&self) -> (Balance, Balance, Balance) {
            let caller = self.env().caller();
            let token1 = self.token1Balance.get(caller).unwrap_or(0);
            let token2 = self.token2Balance.get(caller).unwrap_or(0);
            let myShares = self.shares.get(caller).unwrap_or(0);
            (token1, token2, myShares)
        }
        #[ink(message)]
        pub fn getPoolDetails(&self) -> (Balance, Balance, Balance, Balance) {
            (
                self.totalToken1,
                self.totalToken2,
                self.totalShares,
                self.fees,
            )
        }

        #[ink(message)]
        pub fn provide(
            &mut self,
            _amountToken1: Balance,
            _amountToken2: Balance,
        ) -> Result<Balance, Error> {
            self.validAmountCheck(CheckTypes::token1Balance, _amountToken1)?;
            self.validAmountCheck(CheckTypes::token2Balance, _amountToken2);

            let share;
            if self.totalShares == 0 {
                share = 100 * super::PRECISION;
            } else {
                let share1 = self.totalShares * _amountToken1 / self.totalToken1;
                let share2 = self.totalShares * _amountToken2 / self.totalToken2;

                if share1 != share2 {
                    return Err(Error::NonEquivalentValue);
                }
                share = share1;
            }
            if share == 0 {
                return Err(Error::ThresholdNotReached);
            }
            let caller = self.env().caller();
            let token1 = self.token1Balance.get(caller).unwrap();
            let token2 = self.token2Balance.get(caller).unwrap();
            self.token1Balance.insert(caller, &(token1 - _amountToken1));
            self.token2Balance.insert(caller, &(token2 - _amountToken1));

            self.totalToken1 += _amountToken1;
            self.totalToken2 += _amountToken2;
            self.totalShares += share;

            if self.shares.get(caller).unwrap_or(0) == 0 {
                self.shares.insert(caller, &share);
            } else {
                self.shares
                    .insert(caller, &(self.shares.get(caller).unwrap_or(0) + share));
            }

            Ok(share)
        }

        #[ink(message)]
        pub fn getEquivalentToken1Estimate(
            &self,
            _amountToken2: Balance,
        ) -> Result<Balance, Error> {
            self.activePool();
            Ok(self.totalToken1 * _amountToken2 / self.totalToken2)
        }

        #[ink(message)]
        pub fn getEquivalentToken2Estimate(
            &self,
            _amountToken1: Balance,
        ) -> Result<Balance, Error> {
            self.activePool();
            Ok(self.totalToken2 * _amountToken1 / self.totalToken1)
        }

        #[ink(message)]
        pub fn getWithdrawEstimate(&self, _share: Balance) -> Result<(Balance, Balance), Error> {
            self.activePool();
            if _share > self.totalShares {
                return Err(Error::InvalidShare);
            }

            let amountToken1 = _share * self.totalToken1 / self.totalShares;
            let amountToken2 = _share * self.totalToken2 / self.totalShares;

            Ok((amountToken1, amountToken2))
        }

        #[ink(message)]
        pub fn withdraw(&mut self, _share: Balance) -> Result<(Balance, Balance), Error> {
            let caller = self.env().caller();
            self.validAmountCheck(CheckTypes::shares, _share);

            let (amountToken1, amountToken2) = self.getWithdrawEstimate(_share)?;
            //self.shares.get(caller).insert(|val| *val -= _share);

            self.shares
                .insert(caller, &(self.shares.get(caller).unwrap_or(0) - _share));

            self.totalShares -= _share;

            self.totalToken1 -= amountToken1;
            self.totalToken2 -= amountToken2;

            self.token1Balance.insert(
                caller,
                &(self.token1Balance.get(caller).unwrap_or(0) + amountToken1),
            );

            self.token2Balance.insert(
                caller,
                &(self.token2Balance.get(caller).unwrap_or(0) + amountToken2),
            );

            Ok((amountToken1, amountToken2))
        }

        #[ink(message)]
        pub fn getSwapToken1EstimateGivenToken1(
            &self,
            _amountToken1: Balance,
        ) -> Result<Balance, Error> {
            self.activePool();
            let _amountToken1 = (1000 - self.fees) * _amountToken1 / 1000;

            let token1After = self.totalToken1 + _amountToken1;
            let token2After = self.getK() / token1After;
            let mut amountToken2 = self.totalToken2 - token2After;

            if amountToken2 == self.totalToken2 {
                amountToken2 -= 1;
            }
            Ok(amountToken2)
        }

        #[ink(message)]
        pub fn getSwapToken1EstimateGivenToken2(
            &self,
            _amountToken2: Balance,
        ) -> Result<Balance, Error> {
            self.activePool();

            if _amountToken2 >= self.totalToken2 {
                return Err(Error::InsufficientLiquidity);
            }

            let token2After = self.totalToken2 - _amountToken2;
            let token1After = self.getK() / token2After;
            let amountToken1 = (token1After - self.totalToken1) * 1000 / (1000 - self.fees);

            Ok(amountToken1)
        }

        #[ink(message)]
        pub fn swapToken1GivenToken1(
            &mut self,
            _amountToken1: Balance,
            _mintToken2: Balance,
        ) -> Result<Balance, Error> {
            let caller = self.env().caller();
            self.validAmountCheck(CheckTypes::token1Balance, _amountToken1);

            let amountToken2 = self.getSwapToken1EstimateGivenToken1(_amountToken1)?;
            if amountToken2 < _mintToken2 {
                return Err(Error::SlippageExceeded);
            }

            self.token1Balance.insert(
                caller,
                &(self.token1Balance.get(caller).unwrap_or(0) - _amountToken1),
            );

            self.totalToken1 += _amountToken1;
            self.totalToken2 -= amountToken2;

            self.token2Balance.insert(
                caller,
                &(self.token2Balance.get(caller).unwrap_or(0) + amountToken2),
            );

            Ok(amountToken2)
        }

        #[ink(message)]
        pub fn swapToken1GivenToken2(
            &mut self,
            _amountToken2: Balance,
            _maxToken1: Balance,
        ) -> Result<Balance, Error> {
            let caller = self.env().caller();

            let amountToken1 = self.getSwapToken1EstimateGivenToken2(_amountToken2)?;
            if amountToken1 > _maxToken1 {
                return Err(Error::SlippageExceeded);
            }
            self.validAmountCheck(CheckTypes::token1Balance, amountToken1);

            self.token1Balance.insert(
                caller,
                &(self.token1Balance.get(caller).unwrap_or(0) - amountToken1),
            );

            self.totalToken1 += amountToken1;
            self.totalToken2 -= _amountToken2;

            self.token2Balance.insert(
                caller,
                &(self.token1Balance.get(caller).unwrap_or(0) + _amountToken2),
            );

            Ok(amountToken1)
        }
    }
    #[ink(impl)]
    impl Amm {
        fn validAmountCheck(&self, _balance: CheckTypes, _qty: Balance) -> Result<(), Error> {
            let caller = self.env().caller();
            let my_balance;
            match _balance {
                CheckTypes::shares => {
                    my_balance = self.shares.get(caller).unwrap_or(0);
                }
                CheckTypes::token1Balance => {
                    my_balance = self.token1Balance.get(caller).unwrap_or(0);
                }
                CheckTypes::token2Balance => {
                    my_balance = self.token2Balance.get(caller).unwrap_or(0);
                }
            }

            match _qty {
                0 => Err(Error::ZeroAmount),
                _ if _qty > my_balance => Err(Error::InsufficientAmount),
                _ => Ok(()),
            }
        }

        fn getK(&self) -> Balance {
            self.totalToken1 * self.totalToken2
        }

        fn activePool(&self) -> Result<(), Error> {
            match self.getK() {
                0 => Err(Error::ZeroLiquidity),
                _ => Ok(()),
            }
        }
    }

    /// Unit tests in Rust are normally defined within such a `#[cfg(test)]`
    /// module and test functions are marked with a `#[test]` attribute.
    /// The below code is technically just normal Rust code.
    #[cfg(test)]
    mod tests {
        /// Imports all the definitions from the outer scope so we can use them here.
        use super::*;

        #[ink::test]
        fn new_works() {
            let contract = Amm::new(0);
        }
    }
}
