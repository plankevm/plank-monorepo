use std::{
    marker::PhantomData,
    ops::{Add, AddAssign, RangeTo, RangeToInclusive, Sub, SubAssign},
};

use plank_core::IncIterable;

use crate::op_graph::ValueNodeId;

/// Index from the bottom.
#[repr(transparent)]
#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord)]
pub(crate) struct FromBottom(pub usize);

impl AddAssign<usize> for FromBottom {
    fn add_assign(&mut self, rhs: usize) {
        self.0 += rhs;
    }
}

impl Add<usize> for FromBottom {
    type Output = Self;

    fn add(self, rhs: usize) -> Self::Output {
        FromBottom(self.0 + rhs)
    }
}

impl IncIterable for FromBottom {
    fn get_and_inc(&mut self) -> Self {
        let x = *self;
        self.0 += 1;
        x
    }

    fn dec_and_get(&mut self) -> Self {
        self.0 -= 1;
        *self
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord)]
pub(crate) enum CurrentStack {}

#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord)]
pub(crate) enum TargetStack {}

#[repr(transparent)]
#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord)]
pub(crate) struct Depth<Stack>(pub usize, PhantomData<Stack>);

impl<Stack> Add<usize> for Depth<Stack> {
    type Output = Self;

    fn add(self, rhs: usize) -> Self::Output {
        Depth::new(self.0 + rhs)
    }
}

impl<Stack> Sub<usize> for Depth<Stack> {
    type Output = Self;

    fn sub(self, rhs: usize) -> Self::Output {
        Depth::new(self.0 - rhs)
    }
}

impl<Stack> SubAssign<usize> for Depth<Stack> {
    fn sub_assign(&mut self, rhs: usize) {
        self.0 -= rhs;
    }
}

impl<Stack> Depth<Stack> {
    pub const fn new(i: usize) -> Self {
        Self(i, PhantomData)
    }
}

impl<Stack: Ord + Copy> IncIterable for Depth<Stack> {
    fn get_and_inc(&mut self) -> Self {
        let gotten = *self;
        *self = Depth::new(self.0 + 1);
        gotten
    }

    fn dec_and_get(&mut self) -> Self {
        *self = Depth::new(self.0 - 1);
        *self
    }
}

mod sealed {
    pub trait ToDepthSealed {}
}

impl sealed::ToDepthSealed for FromBottom {}
impl<Stack> sealed::ToDepthSealed for Depth<Stack> {}

pub(crate) trait ToDepth<Stack>: sealed::ToDepthSealed {
    fn to_depth(&self, stack: &[ValueNodeId]) -> Depth<Stack>;
}

impl<Stack: Copy> ToDepth<Stack> for Depth<Stack> {
    fn to_depth(&self, _stack: &[ValueNodeId]) -> Depth<Stack> {
        *self
    }
}

impl<Stack> ToDepth<Stack> for FromBottom {
    #[track_caller]
    fn to_depth(&self, stack: &[ValueNodeId]) -> Depth<Stack> {
        Depth::new(stack.len() - self.0 - 1)
    }
}

pub(crate) trait StackIndex<Stack> {
    type Output<'stack>;

    fn index<'stack>(&self, stack: &'stack [ValueNodeId]) -> Self::Output<'stack>;
}

impl<Stack, I: ToDepth<Stack>> StackIndex<Stack> for I {
    type Output<'s> = ValueNodeId;

    #[track_caller]
    fn index(&self, stack: &[ValueNodeId]) -> Self::Output<'_> {
        stack[self.to_depth(stack).0]
    }
}

impl<Stack, I: ToDepth<Stack>> StackIndex<Stack> for RangeTo<I> {
    type Output<'s> = &'s [ValueNodeId];

    #[track_caller]
    fn index<'s>(&self, stack: &'s [ValueNodeId]) -> Self::Output<'s> {
        &stack[..self.end.to_depth(stack).0]
    }
}

impl<Stack, I: ToDepth<Stack>> StackIndex<Stack> for RangeToInclusive<I> {
    type Output<'s> = &'s [ValueNodeId];

    #[track_caller]
    fn index<'s>(&self, stack: &'s [ValueNodeId]) -> Self::Output<'s> {
        &stack[..=self.end.to_depth(stack).0]
    }
}
