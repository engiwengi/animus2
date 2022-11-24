use bevy::prelude::{Entity, Res, ResMut, Resource};

use super::tick::Tick;

pub struct TimingWheel<'a, S>
where
    S: Resource,
{
    pub scheduler: ResMut<'a, S>,
    pub tick: Res<'a, Tick>,
}

impl<'a, S> TimingWheel<'a, S>
where
    S: Scheduler + Resource,
{
    fn tasks(&mut self) -> impl Iterator<Item = Entity> + '_ {
        self.scheduler.tasks(self.tick.current())
    }

    fn schedule(&mut self, ticks: usize, entity: Entity) {
        debug_assert!(ticks < self.scheduler.capacity());

        self.scheduler.schedule(self.tick.current() + ticks, entity);
    }
}

#[derive(Clone)]
pub enum InnerTimingWheelTree {
    Wheel {
        buckets: Box<[InnerTimingWheelTree; 32]>,
        layer: u32,
    },
    Bucket(Vec<Entity>),
}

impl std::fmt::Debug for InnerTimingWheelTree {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            InnerTimingWheelTree::Wheel { buckets, layer: _ } => {
                write!(f, "{{ ")?;
                for bucket in buckets.iter() {
                    write!(f, "{:?}", bucket)?
                }
                writeln!(f, " }}")
            }

            InnerTimingWheelTree::Bucket(bucket) => bucket.fmt(f),
        }
    }
}

impl InnerTimingWheelTree {
    pub fn new(layers: u32) -> Self {
        let bucket = if let 0 = layers {
            Self::Bucket(Vec::new())
        } else {
            Self::new(layers - 1)
        };

        Self::Wheel {
            buckets: unsafe {
                Box::from_raw(Box::into_raw(vec![bucket; 32].into_boxed_slice())
                    as *mut [InnerTimingWheelTree; 32])
            },
            layer: layers,
        }
    }

    fn bucket(&mut self, tick: usize) -> &mut Vec<Entity> {
        match self {
            InnerTimingWheelTree::Wheel { buckets, layer } => {
                let resolution = buckets.len().pow(*layer);
                let bucket_index = (tick / resolution) % buckets.len();
                buckets[bucket_index].bucket(tick)
            }
            InnerTimingWheelTree::Bucket(bucket) => bucket,
        }
    }
}

impl Scheduler for InnerTimingWheelTree {
    type I<'a> = std::vec::Drain<'a, Entity>;

    fn schedule(&mut self, tick: usize, entity: Entity) {
        self.bucket(tick).push(entity);
    }

    fn tasks(&mut self, tick: usize) -> Self::I<'_> {
        self.bucket(tick).drain(..)
    }

    fn capacity(&self) -> usize {
        match self {
            InnerTimingWheelTree::Wheel { buckets, layer } => buckets.len().pow(*layer + 1),
            InnerTimingWheelTree::Bucket(_) => 1,
        }
    }
}

pub trait Scheduler {
    type I<'a>: Iterator<Item = Entity> + 'a
    where
        Self: 'a;

    fn schedule(&mut self, tick: usize, entity: Entity);

    fn tasks(&mut self, tick: usize) -> Self::I<'_>;

    fn capacity(&self) -> usize;
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test() {
        let mut wheel = InnerTimingWheelTree::new(1);
        wheel.schedule(2, Entity::from_raw(0));

        dbg!(wheel);

        assert!(false);
    }
}
