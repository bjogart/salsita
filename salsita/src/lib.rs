use crate::seal::AnyInput;
use core::any::Any;
use core::any::TypeId;
use std::collections::HashMap;

#[derive(Default)]
pub struct Database {
    cache: HashMap<(TypeId, Box<dyn AnyInput>), Box<dyn Any>>,
}

pub trait Query {
    type Input;

    type Output;

    fn compute(input: Self::Input) -> Self::Output;
}

mod seal {
    use core::any::Any;
    use core::hash::Hash;
    use core::hash::Hasher as _;
    use std::hash;
    use std::hash::DefaultHasher;

    pub trait AnyInput {
        fn eq(&self, other: &dyn AnyInput) -> bool;

        fn hash(&self) -> u64;

        fn as_any(&self) -> &dyn Any;
    }

    impl<T> AnyInput for T
    where
        T: Eq + Hash + 'static,
    {
        fn eq(&self, other: &dyn AnyInput) -> bool {
            other
                .as_any()
                .downcast_ref::<T>()
                .is_some_and(|other| self == other)
        }

        fn hash(&self) -> u64 {
            let mut h = DefaultHasher::new();
            self.hash(&mut h);
            h.finish()
        }

        fn as_any(&self) -> &dyn Any {
            self
        }
    }

    impl PartialEq for dyn AnyInput {
        fn eq(&self, other: &Self) -> bool {
            <Self as AnyInput>::eq(self, other)
        }
    }

    impl Eq for dyn AnyInput {}

    impl Hash for dyn AnyInput {
        fn hash<H>(&self, state: &mut H)
        where
            H: hash::Hasher,
        {
            state.write_u64(<Self as AnyInput>::hash(self))
        }
    }
}

impl Database {
    pub fn query<Q>(&mut self, input: Q::Input) -> Q::Output
    where
        Q: Query + 'static,
        Q::Input: Clone + AnyInput,
        Q::Output: Clone,
    {
        self.cache
            .entry((TypeId::of::<Q>(), Box::new(input.clone())))
            .or_insert_with(|| Box::new(Q::compute(input)))
            .downcast_ref::<Q::Output>()
            .unwrap()
            .clone()
    }
}

#[cfg(test)]
mod test {
    use crate::Database;
    use crate::Query;
    use core::sync::atomic::AtomicUsize;
    use core::sync::atomic::Ordering;

    #[test]
    fn test() {
        static COUNTER: AtomicUsize = AtomicUsize::new(0);

        struct BurritoPrice;
        impl Query for BurritoPrice {
            type Input = ();

            type Output = usize;

            fn compute((): Self::Input) -> Self::Output {
                COUNTER.fetch_add(1, Ordering::SeqCst);
                8
            }
        }

        struct BurritoPriceWithShipping;
        impl Query for BurritoPriceWithShipping {
            type Input = usize;

            type Output = usize;

            fn compute(burrito: Self::Input) -> Self::Output {
                COUNTER.fetch_add(1, Ordering::SeqCst);
                burrito + 2
            }
        }

        struct NumBurritos;
        impl Query for NumBurritos {
            type Input = ();

            type Output = usize;

            fn compute((): Self::Input) -> Self::Output {
                COUNTER.fetch_add(1, Ordering::SeqCst);
                3
            }
        }

        struct TotalPrice;
        impl Query for TotalPrice {
            type Input = (usize, usize);

            type Output = usize;

            fn compute(input: Self::Input) -> Self::Output {
                COUNTER.fetch_add(1, Ordering::SeqCst);
                let (burrito_price_with_shipping, num_burritos) = input;
                burrito_price_with_shipping * num_burritos
            }
        }

        struct SalsaPerBurrito;
        impl Query for SalsaPerBurrito {
            type Input = ();

            type Output = usize;

            fn compute((): Self::Input) -> Self::Output {
                COUNTER.fetch_add(1, Ordering::SeqCst);
                40
            }
        }

        struct SalsaInOrder;
        impl Query for SalsaInOrder {
            type Input = (usize, usize);

            type Output = usize;

            fn compute(input: Self::Input) -> Self::Output {
                COUNTER.fetch_add(1, Ordering::SeqCst);
                let (salsa_per_burrito, num_burritos) = input;
                salsa_per_burrito * num_burritos
            }
        }

        let mut record = Database::default();
        assert_eq!(COUNTER.load(Ordering::SeqCst), 0);

        let burrito_price = record.query::<BurritoPrice>(());
        assert_eq!((COUNTER.load(Ordering::SeqCst), burrito_price), (1, 8));
        let burrito_price_with_shipping = record.query::<BurritoPriceWithShipping>(burrito_price);
        assert_eq!(
            (COUNTER.load(Ordering::SeqCst), burrito_price_with_shipping),
            (2, 10)
        );
        let num_burritos = record.query::<NumBurritos>(());
        assert_eq!((COUNTER.load(Ordering::SeqCst), num_burritos), (3, 3));
        let total_price = record.query::<TotalPrice>((burrito_price_with_shipping, num_burritos));
        assert_eq!((COUNTER.load(Ordering::SeqCst), total_price), (4, 30));
        let salsa_per_burrito = record.query::<SalsaPerBurrito>(());
        assert_eq!((COUNTER.load(Ordering::SeqCst), salsa_per_burrito), (5, 40));
        let salsa_in_order = record.query::<SalsaInOrder>((salsa_per_burrito, num_burritos));
        assert_eq!((COUNTER.load(Ordering::SeqCst), salsa_in_order), (6, 120));

        let total_price = record.query::<TotalPrice>((burrito_price_with_shipping, num_burritos));
        assert_eq!((COUNTER.load(Ordering::SeqCst), total_price), (6, 30));
    }
}
