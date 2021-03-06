use codec::{Decode, Encode};
use rstd::prelude::Vec;
use sr_primitives::traits::{Bounded, Hash};
use support::{
    decl_event, decl_module, decl_storage, dispatch::Result, ensure, StorageMap, StorageValue,
};

use system::ensure_signed;

#[derive(Encode, Decode, Default, Clone, PartialEq)]
#[cfg_attr(feature = "std", derive(Debug))]
pub struct Token<Hash, Balance> {
    pub hash: Hash,
    pub symbol: Vec<u8>,
    pub total_supply: Balance,
}

pub trait Trait: balances::Trait {
    type Event: From<Event<Self>> + Into<<Self as system::Trait>::Event>;
}

decl_event!(
	pub enum Event<T> 
    where 
        <T as system::Trait>::AccountId,
        <T as system::Trait>::Hash,
        <T as balances::Trait>::Balance,
    {
		Issued(AccountId, Hash, Balance),
        Transferd(AccountId, AccountId, Hash, Balance),
        Freezed(AccountId, Hash, Balance),
        UnFreezed(AccountId, Hash, Balance),
	}
);

decl_storage! {
    trait Store for Module<T: Trait> as TokenModule {
        Tokens get(token): map T::Hash => Option<Token<T::Hash, T::Balance>>;
        Owners get(owner): map T::Hash => Option<T::AccountId>;
        BalanceOf get(balance_of): map (T::AccountId, T::Hash) => T::Balance;
        FreeBalanceOf get(free_balance_of): map (T::AccountId, T::Hash) => T::Balance;
        FreezedBalanceOf get(freezed_balance_of): map (T::AccountId, T::Hash) => T::Balance;

        OwnedTokens get(owned_token): map (T::AccountId, u64) => Option<T::Hash>;
        OwnedTokensIndex get(owned_token_index): map T::AccountId => u64;

        Nonce: u64;
    }
}

decl_module! {
    pub struct Module<T: Trait> for enum Call where origin: T::Origin {
        fn deposit_event() = default;

        pub fn issue(origin, symbol: Vec<u8>, total_supply: T::Balance) -> Result {
            Self::do_issue(origin, symbol, total_supply)
        }

        pub fn transfer(origin, token_hash: T::Hash, to: T::AccountId, amount: T::Balance) -> Result {
            let sender = ensure_signed(origin)?;
            Self::do_transfer(sender.clone(), token_hash, to.clone(), amount)?;
            Self::deposit_event(RawEvent::Transferd(sender, to, token_hash, amount));

            Ok(())
        }

        pub fn freeze(origin, hash: T::Hash, amount: T::Balance) -> Result {
            let sender = ensure_signed(origin)?;
            Self::do_freeze(sender, hash, amount)
        }

        pub fn unfreeze(origin, hash: T::Hash, amount: T::Balance) -> Result {
            let sender = ensure_signed(origin)?;
            Self::do_unfreeze(sender, hash, amount)
        }
    }
}

impl<T: Trait> Module<T> {
    pub fn do_issue(origin: T::Origin, symbol: Vec<u8>, total_supply: T::Balance) -> Result {
        let sender = ensure_signed(origin)?;

        let nonce = Nonce::get();

        let hash = (<system::Module<T>>::random_seed(), sender.clone(), nonce)
            .using_encoded(<T as system::Trait>::Hashing::hash);

        let token = Token::<T::Hash, T::Balance> {
            hash: hash.clone(),
            total_supply,
            symbol: symbol.clone(),
        };

        Nonce::mutate(|n| *n += 1);
        Tokens::<T>::insert(hash.clone(), token);
        Owners::<T>::insert(hash.clone(), sender.clone());
        BalanceOf::<T>::insert((sender.clone(), hash.clone()), total_supply);
        FreeBalanceOf::<T>::insert((sender.clone(), hash.clone()), total_supply);

        let owned_token_index = OwnedTokensIndex::<T>::get(sender.clone());
        OwnedTokens::<T>::insert((sender.clone(), owned_token_index), hash);
        OwnedTokensIndex::<T>::insert(sender.clone(), owned_token_index + 1);

        Self::deposit_event(RawEvent::Issued(sender, hash.clone(), total_supply));

        Ok(())
    }

    pub fn do_transfer(
        sender: T::AccountId,
        hash: T::Hash,
        to: T::AccountId,
        amount: T::Balance,
    ) -> Result {
        let token = Self::token(hash);
        ensure!(token.is_some(), "no matching token found");

        ensure!(
            <FreeBalanceOf<T>>::exists((sender.clone(), hash)),
            "sender does not have the token"
        );

        let from_amount = Self::balance_of((sender.clone(), hash.clone()));
        ensure!(from_amount >= amount, "sender does not have enough balance");
        let new_from_amount = from_amount - amount;

        let from_free_amount = Self::free_balance_of((sender.clone(), hash.clone()));
        ensure!(
            from_free_amount >= amount,
            "sender does not have enough free balance"
        );
        let new_from_free_amount = from_free_amount - amount;

        let to_amount = Self::balance_of((to.clone(), hash.clone()));
        let new_to_amount = to_amount + amount;
        ensure!(
            new_to_amount <= T::Balance::max_value(),
            "to amount overflow"
        );

        let to_free_amount = Self::free_balance_of((to.clone(), hash.clone()));
        let new_to_free_amount = to_free_amount + amount;
        ensure!(
            new_to_free_amount <= T::Balance::max_value(),
            "to free amount overflow"
        );

        BalanceOf::<T>::insert((sender.clone(), hash.clone()), new_from_amount);
        FreeBalanceOf::<T>::insert((sender.clone(), hash.clone()), new_from_free_amount);
        BalanceOf::<T>::insert((to.clone(), hash.clone()), new_to_amount);
        FreeBalanceOf::<T>::insert((to.clone(), hash.clone()), new_to_free_amount);

        Ok(())
    }

    pub fn do_freeze(sender: T::AccountId, hash: T::Hash, amount: T::Balance) -> Result {
        let token = Self::token(hash);
        ensure!(token.is_some(), "no matching token found");

        ensure!(
            FreeBalanceOf::<T>::exists((sender.clone(), hash)),
            "sender does not have the token"
        );

        let old_free_amount = Self::free_balance_of((sender.clone(), hash.clone()));
        ensure!(
            old_free_amount >= amount,
            "can not freeze more than available tokens"
        );

        let old_freezed_amount = Self::freezed_balance_of((sender.clone(), hash.clone()));
        ensure!(
            old_freezed_amount + amount <= T::Balance::max_value(),
            "freezed amount overflow"
        );

        FreeBalanceOf::<T>::insert((sender.clone(), hash.clone()), old_free_amount - amount);
        FreezedBalanceOf::<T>::insert((sender.clone(), hash.clone()), old_freezed_amount + amount);

        Self::deposit_event(RawEvent::Freezed(sender, hash, amount));

        Ok(())
    }

    pub fn do_unfreeze(sender: T::AccountId, hash: T::Hash, amount: T::Balance) -> Result {
        let token = Self::token(hash);
        ensure!(token.is_some(), "no matching token found");

        ensure!(
            FreeBalanceOf::<T>::exists((sender.clone(), hash)),
            "sender does not have the token"
        );

        let old_freezed_amount = Self::freezed_balance_of((sender.clone(), hash.clone()));
        ensure!(
            old_freezed_amount >= amount,
            "can not unfreeze more than available tokens"
        );

        let old_free_amount = Self::free_balance_of((sender.clone(), hash.clone()));
        ensure!(
            old_free_amount + amount <= T::Balance::max_value(),
            "unfreezed amount overflow"
        );

        FreeBalanceOf::<T>::insert((sender.clone(), hash.clone()), old_free_amount + amount);
        FreezedBalanceOf::<T>::insert((sender.clone(), hash.clone()), old_freezed_amount - amount);

        Self::deposit_event(RawEvent::UnFreezed(sender, hash, amount));

        Ok(())
    }

    pub fn ensure_free_balance(sender: T::AccountId, hash: T::Hash, amount: T::Balance) -> Result {
        let token = Self::token(hash);
        ensure!(token.is_some(), "no matching token found");

        ensure!(
            FreeBalanceOf::<T>::exists((sender.clone(), hash.clone())),
            "sender does not have the token"
        );

        let free_amount = Self::free_balance_of((sender.clone(), hash.clone()));
        ensure!(
            free_amount >= amount,
            "sender does not have enough free balance"
        );

        Ok(())
    }
}

/// tests for this module
#[cfg(test)]
mod tests {
    use super::*;

    use primitives::{Blake2Hasher, H256};
    use runtime_io::with_externalities;
    use sr_primitives::weights::Weight;
    use sr_primitives::Perbill;
    use sr_primitives::{
        testing::Header,
        traits::{BlakeTwo256, IdentityLookup},
    };
    use std::cell::RefCell;
    use support::{assert_err, assert_ok, impl_outer_origin, parameter_types, traits::Get};

    impl_outer_origin! {
        pub enum Origin for Test {}
    }

    // For testing the module, we construct most of a mock runtime. This means
    // first constructing a configuration type (`Test`) which `impl`s each of the
    // configuration traits of modules we want to use.
    #[derive(Clone, Eq, PartialEq)]
    pub struct Test;
    parameter_types! {
        pub const BlockHashCount: u64 = 250;
        pub const MaximumBlockWeight: Weight = 1024;
        pub const MaximumBlockLength: u32 = 2 * 1024;
        pub const AvailableBlockRatio: Perbill = Perbill::from_percent(75);
        pub const BalancesTransactionBaseFee: u64 = 0;
        pub const BalancesTransactionByteFee: u64 = 0;
    }
    impl system::Trait for Test {
        type Origin = Origin;
        type Call = ();
        type Index = u64;
        type BlockNumber = u64;
        type Hash = H256;
        type Hashing = BlakeTwo256;
        type AccountId = u64;
        type Lookup = IdentityLookup<Self::AccountId>;
        type Header = Header;
        type WeightMultiplierUpdate = ();
        type Event = ();
        type BlockHashCount = BlockHashCount;
        type MaximumBlockWeight = MaximumBlockWeight;
        type MaximumBlockLength = MaximumBlockLength;
        type AvailableBlockRatio = AvailableBlockRatio;
        type Version = ();
    }

    thread_local! {
        static EXISTENTIAL_DEPOSIT: RefCell<u128> = RefCell::new(0);
        static TRANSFER_FEE: RefCell<u128> = RefCell::new(0);
        static CREATION_FEE: RefCell<u128> = RefCell::new(0);
        static BLOCK_GAS_LIMIT: RefCell<u128> = RefCell::new(0);
    }

    pub struct ExistentialDeposit;
    impl Get<u128> for ExistentialDeposit {
        fn get() -> u128 {
            EXISTENTIAL_DEPOSIT.with(|v| *v.borrow())
        }
    }

    pub struct TransferFee;
    impl Get<u128> for TransferFee {
        fn get() -> u128 {
            TRANSFER_FEE.with(|v| *v.borrow())
        }
    }

    pub struct CreationFee;
    impl Get<u128> for CreationFee {
        fn get() -> u128 {
            CREATION_FEE.with(|v| *v.borrow())
        }
    }

    impl balances::Trait for Test {
        type Balance = u128;

        type OnFreeBalanceZero = ();

        type OnNewAccount = ();

        type Event = ();

        type TransactionPayment = ();
        type DustRemoval = ();
        type TransferPayment = ();

        type ExistentialDeposit = ExistentialDeposit;
        type TransferFee = TransferFee;
        type CreationFee = CreationFee;
        type TransactionBaseFee = BalancesTransactionBaseFee;
        type TransactionByteFee = BalancesTransactionByteFee;
        type WeightToFee = ();
    }

    impl super::Trait for Test {
        type Event = ();
    }

    // This function basically just builds a genesis storage key/value store according to
    // our desired mockup.
    fn new_test_ext() -> runtime_io::TestExternalities<Blake2Hasher> {
        system::GenesisConfig::default()
            .build_storage::<Test>()
            .unwrap()
            .into()
    }

    type TokenModule = super::Module<Test>;

    #[test]
    fn token_related_test_case() {
        with_externalities(&mut new_test_ext(), || {
            let ok: Result = Ok(());
            assert_ok!(ok);
            assert_eq!(1u32, 1);
            assert!(true);

            let alice = 10u64;
            let bob = 20u64;
            let charlie = 30u64;

            assert_ok!(TokenModule::issue(
                Origin::signed(alice),
                b"6688".to_vec(),
                21000000
            ));
            assert_eq!(TokenModule::owned_token_index(alice), 1);

            let token_hash = TokenModule::owned_token((alice, 0));
            assert!(token_hash.is_some());
            let token_hash = token_hash.unwrap();
            let token = TokenModule::token(token_hash);
            assert!(token.is_some());
            let token = token.unwrap();

            assert_eq!(TokenModule::balance_of((alice, token.hash)), 21000000);
            assert_eq!(TokenModule::free_balance_of((alice, token.hash)), 21000000);
            assert_eq!(TokenModule::freezed_balance_of((alice, token.hash)), 0);

            assert_ok!(TokenModule::transfer(
                Origin::signed(alice),
                token.hash,
                bob,
                100
            ));
            assert_eq!(TokenModule::balance_of((alice, token.hash)), 20999900);
            assert_eq!(TokenModule::free_balance_of((alice, token.hash)), 20999900);
            assert_eq!(TokenModule::freezed_balance_of((alice, token.hash)), 0);
            assert_eq!(TokenModule::balance_of((bob, token.hash)), 100);
            assert_eq!(TokenModule::free_balance_of((bob, token.hash)), 100);
            assert_eq!(TokenModule::freezed_balance_of((bob, token.hash)), 0);

            assert_err!(
                TokenModule::transfer(Origin::signed(bob), H256::from_low_u64_be(0), charlie, 101),
                "no matching token found"
            );
            assert_err!(
                TokenModule::transfer(Origin::signed(charlie), token.hash, bob, 101),
                "sender does not have the token"
            );
            assert_err!(
                TokenModule::transfer(Origin::signed(bob), token.hash, charlie, 101),
                "sender does not have enough balance"
            );
        });
    }
}
