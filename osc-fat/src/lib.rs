#![allow(dead_code)]
use core::{cell::RefCell, ops::Range, slice};
use osc_block_storage::BlockDevice;
use prim::*;
use std::rc::Rc;

mod math;
pub mod prim;
mod util;
use util::*;

pub struct DirectoryEntriesIterator<'a>(slice::ChunksExact<'a, u8>);

impl<'a> Iterator for DirectoryEntriesIterator<'a> {
    type Item = DirectoryEntry<'a>;

    fn next(&mut self) -> Option<Self::Item> {
        loop {
            let entry = self.0.next()?;

            match entry[0] {
                0x00 => {
                    return None;
                }
                0xE5 => {
                    continue;
                }
                _ => {
                    return Some(entry.into());
                }
            }
        }
    }
}

pub enum DirectoryEntry<'a> {
    Standard(StandardDirectoryEntry<'a>),
    LongFileName(LongFileNameEntry<'a>),
}

impl<'a> DirectoryEntry<'a> {
    pub const SIZE: usize = 32;
}

impl<'a> From<&'a [u8]> for DirectoryEntry<'a> {
    fn from(other: &'a [u8]) -> Self {
        if other[11] == 0x0F {
            Self::LongFileName(LongFileNameEntry(other))
        } else {
            Self::Standard(StandardDirectoryEntry(other))
        }
    }
}

pub struct StandardDirectoryEntry<'a>(&'a [u8]);

impl<'a> StandardDirectoryEntry<'a> {
    const RANGE_NAME: ByteRange = 0..8;
    const RANGE_EXT: ByteRange = 8..11;
    const RANGE_ATTR: ByteRange = 11..12;
    const RANGE_RESERVED_WINNT: ByteRange = 12..13;
    const RANGE_CREATION_TIME_DECISECS: ByteRange = 13..14;
    const RANGE_CREATION_TIME: ByteRange = 14..16;
    const RANGE_CREATION_DATE: ByteRange = 16..18;
    const RANGE_ACCESS_DATE: ByteRange = 18..20;
    const RANGE_FIRST_CLUSTER_HIGH: ByteRange = 20..22;
    const RANGE_MOD_TIME: ByteRange = 22..24;
    const RANGE_MOD_DATE: ByteRange = 24..26;
    const RANGE_FIRST_CLUSTER_LOW: ByteRange = 26..28;
    const RANGE_SIZE: ByteRange = 28..32;

    pub fn name(&self) -> &[u8] {
        self.0.range(Self::RANGE_NAME)
    }

    pub fn ext(&self) -> &[u8] {
        self.0.range(Self::RANGE_EXT)
    }

    pub fn size(&self) -> u32 {
        self.0.u32(Self::RANGE_SIZE)
    }

    pub fn is_read_only(&self) -> bool {
        self.0.u8(Self::RANGE_ATTR) & 0x01 != 0
    }

    pub fn is_hidden(&self) -> bool {
        self.0.u8(Self::RANGE_ATTR) & 0x02 != 0
    }

    pub fn is_system(&self) -> bool {
        self.0.u8(Self::RANGE_ATTR) & 0x04 != 0
    }

    pub fn is_volume_id(&self) -> bool {
        self.0.u8(Self::RANGE_ATTR) & 0x08 != 0
    }

    pub fn is_directory(&self) -> bool {
        self.0.u8(Self::RANGE_ATTR) & 0x10 != 0
    }

    pub fn is_archive(&self) -> bool {
        self.0.u8(Self::RANGE_ATTR) & 0x20 != 0
    }

    pub fn first_cluster_high(&self) -> u16 {
        self.0.u16(Self::RANGE_FIRST_CLUSTER_HIGH)
    }

    pub fn first_cluster_low(&self) -> u16 {
        self.0.u16(Self::RANGE_FIRST_CLUSTER_LOW)
    }

    pub fn first_cluster(&self) -> u32 {
        ((self.first_cluster_high() as u32) << 16) | (self.first_cluster_low() as u32)
    }
}

pub struct LongFileNameEntry<'a>(&'a [u8]);

impl<'a> LongFileNameEntry<'a> {
    const RANGE_ORDER: ByteRange = 0..1;
    const RANGE_PORTION1: ByteRange = 1..11;
    const RANGE_ATTR: ByteRange = 11..12;
    const RANGE_LONG_ENTRY_TYPE: ByteRange = 12..13;
    const RANGE_CHECKSUM: ByteRange = 13..14;
    const RANGE_PORTION2: ByteRange = 14..26;
    const RANGE_ZERO: ByteRange = 26..28;
    const RANGE_PORTION3: ByteRange = 28..32;

    pub fn chars(&self) -> LongFileNameCharIterator {
        LongFileNameCharIterator::new(self)
    }

    fn portion1(&self) -> &[u8] {
        self.0.range(Self::RANGE_PORTION1)
    }

    fn portion2(&self) -> &[u8] {
        self.0.range(Self::RANGE_PORTION2)
    }

    fn portion3(&self) -> &[u8] {
        self.0.range(Self::RANGE_PORTION3)
    }
}

pub struct LongFileNameCharIterator<'a> {
    entry: &'a LongFileNameEntry<'a>,
    state: LongFileNameCharIteratorState<'a>,
}

impl<'a> LongFileNameCharIterator<'a> {
    fn new(entry: &'a LongFileNameEntry) -> Self {
        LongFileNameCharIterator {
            entry,
            state: LongFileNameCharIteratorState::Portion1(U16Iterator(entry.portion1().iter())),
        }
    }
}

impl<'a> Iterator for LongFileNameCharIterator<'a> {
    type Item = u16;

    fn next(&mut self) -> Option<Self::Item> {
        use LongFileNameCharIteratorState::*;

        loop {
            match self.state {
                Portion1(ref mut inner) => match inner.next() {
                    Some(0) => {
                        return None;
                    }
                    Some(v) => {
                        return Some(v);
                    }
                    None => {
                        self.state = Portion2(U16Iterator(self.entry.portion2().iter()));
                    }
                },
                Portion2(ref mut inner) => match inner.next() {
                    Some(0) => {
                        return None;
                    }
                    Some(v) => {
                        return Some(v);
                    }
                    None => {
                        self.state = Portion3(U16Iterator(self.entry.portion3().iter()));
                    }
                },
                Portion3(ref mut inner) => match inner.next() {
                    Some(0) => {
                        return None;
                    }
                    Some(v) => {
                        return Some(v);
                    }
                    None => {
                        return None;
                    }
                },
            }
        }
    }
}

enum LongFileNameCharIteratorState<'a> {
    Portion1(U16Iterator<'a>),
    Portion2(U16Iterator<'a>),
    Portion3(U16Iterator<'a>),
}

struct U16Iterator<'a>(slice::Iter<'a, u8>);

impl<'a> Iterator for U16Iterator<'a> {
    type Item = u16;

    fn next(&mut self) -> Option<Self::Item> {
        match self.0.next() {
            None => None,
            Some(first_byte) => match self.0.next() {
                None => panic!("Incomplete number of bytes"),
                Some(second_byte) => Some((*second_byte as u16) << 8 | (*first_byte as u16)),
            },
        }
    }
}

#[derive(Debug, Copy, Clone)]
pub enum Variant {
    Fat12,
    Fat16,
    Fat32,
}

impl Variant {
    pub fn from_cluster_count(cluster_count: u32) -> Self {
        if cluster_count < 4085 {
            Self::Fat12
        } else if cluster_count < 65525 {
            Self::Fat16
        } else {
            Self::Fat32
        }
    }
}

pub struct ReadBuffer<'a> {
    device: Rc<RefCell<Box<dyn BlockDevice>>>,
    buffer: &'a mut [u8],
    sector_size_bytes: u16,
    loaded_sectors: Option<Range<u64>>,
}

impl<'a> ReadBuffer<'a> {
    fn new(
        device: Rc<RefCell<Box<dyn BlockDevice>>>,
        buffer: &'a mut [u8],
        sector_size_bytes: u16,
    ) -> Self {
        Self {
            device,
            buffer,
            sector_size_bytes,
            loaded_sectors: None,
        }
    }

    pub fn get_sector(&mut self, sector_index: u64) -> &[u8] {
        let sector_range = self.ensure_sector_prime(sector_index);
        &self.buffer[sector_range]
    }

    pub fn get_loaded_sector(&self, sector_index: u64) -> Option<&[u8]> {
        match self.loaded_sectors {
            Some(ref loaded_sectors) if loaded_sectors.contains(&sector_index) => {
                let sector_range = self.sector_range(loaded_sectors, sector_index);
                return Some(&self.buffer[sector_range]);
            }
            Some(_) | None => {
                return None;
            }
        }
    }

    pub fn ensure_sector(&mut self, sector_index: u64) {
        self.ensure_sector_prime(sector_index);
    }

    fn ensure_sector_prime(&mut self, sector_index: u64) -> Range<usize> {
        match self.loaded_sectors {
            Some(ref loaded_sectors) if loaded_sectors.contains(&sector_index) => {
                return self.sector_range(loaded_sectors, sector_index);
            }
            Some(_) | None => {
                return self.read_block_for_sector(sector_index);
            }
        }
    }

    fn sector_range(&self, loaded_sectors: &Range<u64>, sector_index: u64) -> Range<usize> {
        // NOTE: this could technically truncate on a 32-bit system, but in practice it
        // won't because the buffer size can't be big enough that a relative sector
        // index can be big enough to do that
        let relative_sector_index = (sector_index - loaded_sectors.start) as usize;

        let sector_size_bytes = usize::from(self.sector_size_bytes);
        let byte_start = relative_sector_index * sector_size_bytes;
        let byte_end = byte_start + sector_size_bytes;

        byte_start..byte_end
    }

    fn read_block_for_sector(&mut self, desired_sector_index: u64) -> Range<usize> {
        let mut device = self.device.borrow_mut();

        let sector_size_bytes = u64::from(self.sector_size_bytes);
        let block_size_bytes = u64::from(device.block_size());

        // Read the block containing the desired sector
        let block_index = (desired_sector_index * sector_size_bytes) / block_size_bytes;
        let blocks_read = device.read_blocks(block_index, self.buffer);
        let sectors_read = (blocks_read * block_size_bytes) / sector_size_bytes;

        // TODO: this means the sector doesn't exist on disk, we need
        // an error handling strategy for things like that
        assert_ne!(0, sectors_read);

        let first_sector = (block_index * block_size_bytes) / sector_size_bytes;
        let last_sector = first_sector + sectors_read;

        let loaded_sectors = first_sector..last_sector;
        let sector_range = self.sector_range(&loaded_sectors, desired_sector_index);

        self.loaded_sectors = Some(loaded_sectors);
        sector_range
    }
}

pub struct ClusterWalker<'a> {
    buffer: ReadBuffer<'a>,
    cluster_index: u32,
    cluster_sector_index: u8,
    geo: FATGeometry,
}

impl<'a> ClusterWalker<'a> {
    fn open(buffer: ReadBuffer<'a>, cluster_index: u32, geo: FATGeometry) -> Option<Self> {
        let mut result = Self {
            buffer,
            cluster_index,
            cluster_sector_index: 0,
            geo,
        };

        result.ensure_sector();

        Some(result)
    }

    pub fn current_sector(&self) -> &[u8] {
        self.buffer
            .get_loaded_sector(self.absolute_sector_index())
            .unwrap_or_else(|| unreachable!())
    }

    pub fn next_sector(&mut self) -> bool {
        match self.cluster_sector_index + 1 {
            n if n == self.geo.cluster_size_sectors => false,
            n => {
                self.cluster_sector_index = n;
                self.ensure_sector();
                true
            }
        }
    }

    pub fn next_cluster(mut self) -> Option<Self> {
        let fat_byte_offset = u64::from(self.cluster_index) * 4;

        let fat_sector =
            self.geo.first_fat_sector + (fat_byte_offset / u64::from(self.geo.sector_size_bytes));

        // Sector size bytes has a maximum value of 4096 so 'as' is safe here
        let ent_offset = (fat_byte_offset % u64::from(self.geo.sector_size_bytes)) as u32;

        let fat_sector_data = self.buffer.get_sector(fat_sector);

        match FileAllocationTable32::from(fat_sector_data).get_entry(ent_offset) {
            FileAllocationTable32Result::NextClusterIndex(next_cluster_index) => {
                self.cluster_index = next_cluster_index;
                self.ensure_sector();
                Some(self)
            }
            FileAllocationTable32Result::EndOfChain => None,
            FileAllocationTable32Result::BadCluster => unimplemented!(),
        }
    }

    fn absolute_sector_index(&self) -> u64 {
        let absolute_start_sector_index = u64::from(self.cluster_index - 2)
            * u64::from(self.geo.cluster_size_sectors)
            + self.geo.first_data_sector;

        let absolute_sector_index =
            absolute_start_sector_index + u64::from(self.cluster_sector_index);

        absolute_sector_index
    }

    fn ensure_sector(&mut self) {
        // TODO: this should be fallible
        self.buffer.ensure_sector(self.absolute_sector_index());
    }
}

pub struct DirectoryWalker<'a> {
    cluster_walker: ClusterWalker<'a>,
}

impl<'a> DirectoryWalker<'a> {
    fn new(cluster_walker: ClusterWalker<'a>) -> Self {
        Self { cluster_walker }
    }

    pub fn occupied_entries(&self) -> DirectoryEntriesIterator<'_> {
        DirectoryEntriesIterator(
            self.cluster_walker
                .current_sector()
                .chunks_exact(DirectoryEntry::SIZE),
        )
    }

    pub fn next(mut self) -> Option<Self> {
        if self.cluster_walker.next_sector() {
            return Some(self);
        }

        self.cluster_walker
            .next_cluster()
            .map(|new_cluster_walker| Self {
                cluster_walker: new_cluster_walker,
            })
    }

    pub fn enumerate_occupied_entries<F>(self, mut func: F)
    where
        F: FnMut(DirectoryEntry<'_>),
    {
        let mut walker = self;

        loop {
            for entry in walker.occupied_entries() {
                func(entry)
            }

            if let Some(new_walker) = walker.next() {
                walker = new_walker;
            } else {
                break;
            }
        }
    }
}

#[derive(Debug, Clone, Copy)]
struct FATGeometry {
    cluster_size_sectors: u8,
    sector_size_bytes: u16,
    first_fat_sector: u64,
    first_data_sector: u64,
}

pub type Cluster = u32;

pub type DirectoryInitialCluster = Cluster;

pub enum DirectorySelector {
    Root,
    Normal(DirectoryInitialCluster),
}

pub struct FATFileSystem {
    device: Rc<RefCell<Box<dyn BlockDevice>>>,
    device_block_size: u16,

    variant: Variant,
    geo: FATGeometry,

    // TODO: Fat32 only
    root_cluster: u32,
}

impl FATFileSystem {
    pub fn open(mut device: Box<dyn BlockDevice>) -> Self {
        use std::str;

        // Read the BPB
        let mut read_buffer = [0u8; 512];
        device.read_blocks(0, &mut read_buffer);

        let read_buffer_slice = &read_buffer[..];

        // Right, what version of FAT are we dealing with?
        let bpb: CommonBiosParameterBlock = read_buffer_slice.into();

        let bytes_per_sector = bpb.bytes_per_sector();
        let root_dir_sector_count =
            root_dir_sector_count(bpb.root_entry_count() as u32, bytes_per_sector);

        let sectors_per_fat = sectors_per_fat(read_buffer_slice);
        let sectors_per_cluster = bpb.sectors_per_cluster();
        let reserved_sectors = bpb.reserved_sector_count();

        let meta_sectors = meta_sector_count(
            reserved_sectors,
            sectors_per_fat,
            bpb.fat_count(),
            root_dir_sector_count,
        );

        let first_data_sector = meta_sectors;

        let data_sectors = bpb.total_sectors() - meta_sectors;

        let count_of_clusters = data_sectors / u32::from(sectors_per_cluster);

        let variant = Variant::from_cluster_count(count_of_clusters);

        let root_cluster = match variant {
            Variant::Fat12 | Variant::Fat16 => {
                unimplemented!();
            }

            Variant::Fat32 => {
                ExtendedFat32BiosParameterBlock::from(read_buffer_slice).root_cluster()
            }
        };

        println!(
            "Variant: {:?}, OEM: {}",
            variant,
            str::from_utf8(bpb.oem()).unwrap()
        );

        let geo = FATGeometry {
            cluster_size_sectors: sectors_per_cluster,
            sector_size_bytes: bytes_per_sector,
            first_fat_sector: reserved_sectors.into(),
            first_data_sector: first_data_sector.into(),
        };

        Self {
            device_block_size: device.block_size(),
            device: Rc::new(RefCell::new(device)),

            variant,
            root_cluster,
            geo,
        }
    }

    pub fn required_read_buffer_size(&self) -> usize {
        core::cmp::max(
            usize::from(self.geo.sector_size_bytes),
            usize::from(self.device_block_size),
        )
    }

    pub fn walk_directory<'a>(
        &self,
        buffer: &'a mut [u8],
        directory: DirectorySelector,
    ) -> DirectoryWalker<'a> {
        let buffer = ReadBuffer::new(self.device.clone(), buffer, self.geo.sector_size_bytes);

        let cluster_walker = match directory {
            DirectorySelector::Normal(cluster_index) => {
                ClusterWalker::open(buffer, cluster_index, self.geo).unwrap()
            }
            DirectorySelector::Root => match self.variant {
                Variant::Fat12 | Variant::Fat16 => {
                    unimplemented!();
                }

                Variant::Fat32 => ClusterWalker::open(buffer, self.root_cluster, self.geo).unwrap(),
            },
        };

        let dir_walker = DirectoryWalker::new(cluster_walker);
        dir_walker
    }

    pub fn read<'a>(&mut self, file_first_cluster: u32, cluster_buffer: &'a mut [u8]) {
        let first_sector = first_sector_of_cluster(
            file_first_cluster,
            self.geo.cluster_size_sectors,
            self.geo.first_data_sector as u32,
        ) as u64;
        self.device
            .borrow_mut()
            .read_blocks(first_sector, cluster_buffer);
    }
}
