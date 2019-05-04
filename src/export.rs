pub auto trait DataNotCode {}

macro_rules! impl_ {
    ($($T:ident),*) => {
        impl<R, $($T),*> !DataNotCode for fn($($T),*) -> R {}
        impl<R, $($T),*> !DataNotCode for extern "C" fn($($T),*) -> R {}
    };
}

macro_rules! impls {
    ($head:ident $(,$tail:ident)*) => {
        impls!($($tail),*);
        impl_!($head $(,$tail)*);
    };

    () => {
        impl_!();
    };
}

// FIXME this should use Variadic Generics
impls!(A, B, C, D, E, F, G, H, I, J, K, L);

pub fn is_data<T>()
where
    T: DataNotCode + ?Sized,
{
}
