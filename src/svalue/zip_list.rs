use std::mem;
use std::iter::Chain;
use std::iter::Cloned;
use std::slice;
use crate::util::bytes_to_i64;

const ZIP_LIST_I16_ENC: u8 = 0b1100_0000;
const ZIP_LIST_I32_ENC: u8 = 0b1101_0000;
const ZIP_LIST_I64_ENC: u8 = 0b1110_0000;
const ZIP_LIST_I24_ENC: u8 = 0b1111_0000;
const ZIP_LIST_I8_ENC: u8 = 0b1111_1110;

#[derive(Clone)]
enum Encoding {
    Str(usize),
    Int(i64),
}

impl Encoding {
    fn unwrap_str(&self) -> usize {
        match self {
            Self::Str(sz) => *sz,
            _ => panic!("this is an int encoding"),
        }
    }

    fn unwrap_int(&self) -> i64 {
        match self {
            Self::Int(v) => *v,
            _ => panic!("this is a str encoding"),
        }
    }

    fn is_str(&self) -> bool {
        match self {
            Self::Str(_) => true,
            _ => false,
        }
    }

    fn is_int(&self) -> bool {
        !self.is_str()
    }

    fn blob_len(&self) -> usize {
        match self {
            Self::Str(sz) => {
                if *sz < 1 << 6 {
                    return 1;
                }
                if *sz < 1 << 14 {
                    return 2;
                }
                assert!(*sz < 1 << 32);
                5
            }
            Self::Int(v) => {
                if *v > 0 && *v < 12 {
                    return 1;
                }
                if *v > std::i8::MIN as i64 && *v < std::i8::MAX as i64 {
                    return 1 + mem::size_of::<i8>();
                }
                if *v > std::i16::MIN as i64 && *v < std::i16::MAX as i64 {
                    return 1 + mem::size_of::<i16>();
                }
                if *v > -(1 << 23) && *v < (1 << 23 - 1) {
                    return 1 + 3;
                }
                if *v > std::i32::MIN as i64 && *v < std::i32::MAX as i64 {
                    return 1 + mem::size_of::<i32>();
                }
                1 + mem::size_of::<i64>()
            }
        }
    }

    fn blob_len_with_content(&self) -> usize {
        match self {
            Self::Str(sz) => self.blob_len() + *sz,
            Self::Int(_) => self.blob_len(),
        }
    }

    fn index(&self, idx: usize) -> u8 {
        match self {
            Self::Str(_) => self.index_str(idx),
            Self::Int(_) => self.index_int(idx),
        }
    }

    fn index_str(&self, idx: usize) -> u8 {
        let len = self.blob_len();
        let mut v = 0;
        assert!(idx < len);
        if idx == 0 {
            match len {
                2 => v |= 0b0100_0000,
                5 => {
                    return 0b1000_0000;
                }
                _ => {}
            }
        }
        v |= (self.unwrap_str() >> ((len - idx - 1) * 8)) & 0xff;
        v as u8
    }

    fn index_int(&self, idx: usize) -> u8 {
        assert!(idx < self.blob_len());
        if let Self::Int(v) = self {
            if idx == 0 {
                if *v > 0 && *v < 12 {
                    return *v as u8 | 0b1111_0000;
                }
                if *v > std::i8::MIN as i64 && *v < std::i8::MAX as i64 {
                    return ZIP_LIST_I8_ENC;
                }
                if *v > std::i16::MIN as i64 && *v < std::i16::MAX as i64 {
                    return ZIP_LIST_I16_ENC;
                }
                if *v > -(1 << 23) && *v < (1 << 23 - 1) {
                    return ZIP_LIST_I24_ENC;
                }
                if *v > std::i32::MIN as i64 && *v < std::i32::MAX as i64 {
                    return ZIP_LIST_I32_ENC;
                }
                return ZIP_LIST_I64_ENC;
            }
            return ((*v >> (self.blob_len() - idx - 1) as i64 * 8) & 0xff) as u8;
        }
        panic!("This is not a str encoding")
    }

    fn iter(&self) -> EncodingIter {
        EncodingIter {
            enc: self.clone(),
            curr: 0,
        }
    }

    fn iter_with_content<'a>(&'a self, content: &'a [u8])
                             -> Chain<EncodingIter, Cloned<slice::Iter<u8>>> {
        match self {
            Self::Str(_) => self.iter().chain(content.iter().cloned()),
            Self::Int(_) => self.iter().chain("".as_bytes().iter().cloned()),
        }
    }

    fn is_str_enc(x: &[u8]) -> bool {
        x[0] & 0b1100_0000 != 0b1100_0000
    }

    fn is_int_enc(x: &[u8]) -> bool {
        !Self::is_str_enc(x)
    }

    fn parse(x: &[u8]) -> Encoding {
        match Self::is_str_enc(x) {
            true => Self::parse_str_enc(x),
            false => Self::parse_int_enc(x),
        }
    }

    fn parse_str_enc(x: &[u8]) -> Encoding {
        let sz = match x[0] & 0b1100_0000 {
            0b0000_0000 => 1usize,
            0b0100_0000 => 2usize,
            0b1000_0000 => 5usize,
            _ => panic!("not possible"),
        };
        let mut v = x[0] as usize & 0b0011_1111;
        for i in 1..sz {
            v <<= 8;
            v |= x[i] as usize;
        }
        Self::Str(v)
    }

    fn parse_int_enc(x: &[u8]) -> Encoding {
        let sz = match x[0] {
            ZIP_LIST_I16_ENC => mem::size_of::<i16>(),
            ZIP_LIST_I32_ENC => mem::size_of::<i32>(),
            ZIP_LIST_I64_ENC => mem::size_of::<i64>(),
            ZIP_LIST_I24_ENC => 3,
            ZIP_LIST_I8_ENC => mem::size_of::<i8>(),
            _ => {
                if x[0] >> 4 != 0b1111 {
                    panic!("not int encoding");
                }
                let k = x[0] & 0x0f;
                assert!(k > 0 && k < 12);
                return Self::Int(k as i64);
            }
        };
        let mut v = if x[1] >> 7 == 1 {
            -1i64
        } else {
            0i64
        };
        for i in 0..sz {
            v <<= 8;
            v |= x[i + 1] as i64;
        }
        Self::Int(v)
    }

    fn write_with_content(&self, dst: &mut [u8], content: &[u8]) {
        assert_eq!(self.blob_len_with_content(), dst.len());
        for p in dst.iter_mut().zip(self.iter_with_content(content)) {
            *p.0 = p.1;
        }
    }
}

fn encode_prev_length(len: usize, idx: usize) -> Option<u8> {
    if len < 254 {
        if idx != 0 {
            return None;
        }
        return Some(len as u8);
    }
    if len < std::u32::MAX as usize {
        if idx == 0 {
            return Some(0xfe);
        }
        if idx < 5 {
            return Some(((len >> (4 - idx) * 8) & 0xff) as u8);
        }
    }
    None
}

fn prev_length_size(len: usize) -> usize {
    if len < 254 {
        1
    } else {
        5
    }
}

fn decode_prev_length(x: &[u8]) -> usize {
    if x[0] != 0xfe {
        return x[0] as usize;
    }
    let mut v = 0;
    for i in 1..5 {
        v <<= 8;
        v |= x[i] as usize;
    }
    v
}

fn prev_length_iter(len: usize) -> LengthIter {
    LengthIter(0, len)
}

fn write_prev_length(len: usize, x: &mut [u8]) {
    assert_eq!(prev_length_size(len), x.len());
    for p in x.iter_mut().zip(prev_length_iter(len)) {
        *p.0 = p.1;
    }
}

fn force_write_large_prev_length(len: usize, x: &mut [u8]) {
    assert_eq!(x.len(), 5);
    assert!(len < 254);
    x[4] = len as u8;
}

struct LengthIter(usize, usize);

impl Iterator for LengthIter {
    type Item = u8;

    fn next(&mut self) -> Option<Self::Item> {
        self.0 += 1;
        encode_prev_length(self.1, self.0 - 1)
    }
}


struct EncodingIter {
    enc: Encoding,
    curr: usize,
}

impl Iterator for EncodingIter {
    type Item = u8;

    fn next(&mut self) -> Option<Self::Item> {
        if self.curr < self.enc.blob_len() {
            self.curr += 1;
            Some(self.enc.index(self.curr - 1))
        } else {
            None
        }
    }
}

pub enum ZipListValue<'a> {
    Bytes(&'a [u8]),
    Int(i64),
}

impl<'a> ZipListValue<'a> {
    fn unwrap_bytes(&self) -> &'a [u8] {
        match self {
            Self::Bytes(s) => *s,
            _ => panic!("fail unwrapping to bytes"),
        }
    }

    fn unwrap_int(&self) -> i64 {
        match self {
            Self::Int(k) => *k,
            _ => panic!("fail unwrapping to int"),
        }
    }
}

impl<'a> PartialEq<&[u8]> for ZipListValue<'a> {
    fn eq(&self, other: &&[u8]) -> bool {
        match self {
            ZipListValue::Bytes(b) => *b == *other,
            ZipListValue::Int(i) => {
                i.to_string().as_bytes() == *other
            }
        }
    }
}

struct Node<'a> {
    prev_raw_len: usize,
    prev_raw_len_size: usize,
    encoding: Encoding,
    content: &'a [u8],
}

impl<'a> Node<'a> {
    fn new(x: &'a [u8]) -> Node<'a> {
        let prev_raw_len = decode_prev_length(x);
        let prev_raw_len_size = prev_length_size(prev_raw_len);
        let encoding = Encoding::parse(&x[prev_raw_len_size..]);
        Node {
            prev_raw_len,
            prev_raw_len_size,
            encoding,
            content: x,
        }
    }

    fn header_size(&self) -> usize {
        self.prev_raw_len_size + self.encoding.blob_len()
    }

    fn blob_len(&self) -> usize {
        self.prev_raw_len_size + self.encoding.blob_len_with_content()
    }

    fn parse_blob_len(x: &[u8]) -> usize {
        let prev_raw_len = decode_prev_length(x);
        let prev_raw_len_size = prev_length_size(prev_raw_len);
        let encoding = Encoding::parse(&x[prev_raw_len_size..]);
        prev_raw_len_size + encoding.blob_len_with_content()
    }

    fn value(&self) -> ZipListValue<'a> {
        match self.encoding {
            Encoding::Int(i) => ZipListValue::Int(i),
            Encoding::Str(sz) =>
                ZipListValue::Bytes(&self.content[self.header_size()..self.header_size() + sz]),
        }
    }
}


pub struct ZipListNodeMut<'a> {
    list: &'a mut ZipList,
    off: usize,
}

impl<'a> ZipListNodeMut<'a> {
    pub fn at_end(&self) -> bool {
        self.off == self.list.0.len()
    }

    pub fn at_begin(&self) -> bool {
        self.off == self.list.header_len()
    }

    pub fn value(&'a self) -> ZipListValue<'a> {
        if self.at_end() {
            panic!("can't get value at the end of zip_list");
        }
        Node::new(&self.list.0[self.off..]).value()
    }

    pub fn insert(self, x: &[u8]) -> ZipListNodeMut<'a> {
        self.list.inner_insert(self.off, x);
        self
    }

    pub fn delete(self) -> ZipListNodeMut<'a> {
        if self.at_end() {
            panic!("can't delete at the end of zip_list");
        }
        self.list.inner_delete(self.off, 1);
        self
    }

    pub fn delete_range(self, num: usize) -> ZipListNodeMut<'a> {
        if self.at_end() {
            panic!("can't delete at the end of zip_list");
        }
        self.list.inner_delete(self.off, num);
        self
    }

    pub fn move_next(mut self) -> ZipListNodeMut<'a> {
        if self.at_end() {
            panic!("can't move_next at the end of zip_list");
        }
        self.off += Node::parse_blob_len(&self.list.0[self.off..]);
        self
    }

    pub fn move_prev(mut self) -> ZipListNodeMut<'a> {
        if self.at_begin() {
            panic!("can't move_prev at the begin of zip_list")
        }
        self.off -= decode_prev_length(&self.list.0[self.off..]);
        self
    }

    pub fn delete_first_n_filter<F>(mut self, mut n: usize, f: F) -> ZipListNodeMut<'a>
        where F: Fn(&ZipListValue) -> bool {
        while !self.at_end() {
            if f(&self.value()) {
                self = self.delete();
                n -= 1;
            } else {
                self = self.move_next();
            }
            if n == 0 {
                break;
            }
        }
        self
    }

    pub fn delete_last_n_filter<F>(mut self, mut n: usize, f: F) -> ZipListNodeMut<'a>
        where F: Fn(&ZipListValue) -> bool {
        loop {
            if n == 0 {
                break;
            }
            if f(&self.value()) {
                self = self.delete();
                n -= 1;
                if self.at_begin() {
                    break;
                }
                if self.at_end() {
                    self = self.list.tail_mut();
                }
            } else {
                if self.at_begin() {
                    break;
                }
                self = self.move_prev();
            }
        }
        self
    }
}

pub struct Iter<'a> {
    list: &'a ZipList,
    off: usize,
}

impl<'a> Iterator for Iter<'a> {
    type Item = ZipListValue<'a>;

    fn next(&mut self) -> Option<Self::Item> {
        let off = self.off;
        let n = self.list.get_value(off);
        match n {
            None => None,
            Some(e) => {
                self.off += Node::parse_blob_len(&self.list.0[off..]);
                Some(e)
            }
        }
    }
}

pub struct IterRev<'a> {
    list: &'a ZipList,
    off: usize,
}

impl<'a> Iterator for IterRev<'a> {
    type Item = ZipListValue<'a>;

    fn next(&mut self) -> Option<Self::Item> {
        let off = self.off;
        if off == self.list.0.len() || off == 0 {
            return None;
        }
        let prev_len = decode_prev_length(&self.list.0[off..]);
        self.off = if prev_len == 0 {
            0
        } else {
            off - prev_len
        };
        Some(Node::new(&self.list.0[off..]).value())
    }
}


// ZipList
// | tail offset: sizeof(usize) | number of nodes: sizeof(u16) | node 1 | node 2 | ... | node N |
pub struct ZipList(Vec<u8>);

const ZIP_LIST_TAIL_OFF_SIZE: usize = mem::size_of::<usize>();
const ZIP_LIST_LEN_SIZE: usize = mem::size_of::<u16>();
const ZIP_LIST_HEADER_SIZE: usize = mem::size_of::<usize>() + mem::size_of::<u16>();

impl ZipList {
    pub fn new() -> ZipList {
        let mut zl = ZipList(vec![0; ZIP_LIST_HEADER_SIZE]);
        zl.set_tail_offset(zl.blob_len());
        zl.set_len(0);
        zl
    }

    pub fn from_bytes(bytes: Vec<u8>) -> ZipList {
        ZipList(bytes)
    }

    pub fn raw_slice(&self) -> &[u8] {
        &self.0[..]
    }

    pub fn blob_len(&self) -> usize {
        self.0.len()
    }

    pub fn len(&self) -> usize {
        let l = self.get_usize_value(ZIP_LIST_TAIL_OFF_SIZE,
                                     ZIP_LIST_LEN_SIZE);
        assert!(l < std::u16::MAX as usize);
        l
    }

    pub fn push(&mut self, content: &[u8]) -> ZipListNodeMut {
        self.inner_insert(self.0.len(), content);
        self.get_node_mut(self.get_tail_offset())
    }

    pub fn iter(&self) -> Iter {
        Iter {
            list: self,
            off: self.header_len(),
        }
    }

    pub fn iter_rev(&self) -> IterRev {
        IterRev {
            list: self,
            off: self.get_tail_offset(),
        }
    }

    pub fn front(&self) -> Option<ZipListValue> {
        self.get_value(self.header_len())
    }

    pub fn front_mut(&mut self) -> ZipListNodeMut {
        self.get_node_mut(self.header_len())
    }

    pub fn tail(&self) -> Option<ZipListValue> {
        self.get_value(self.get_tail_offset())
    }

    pub fn tail_mut(&mut self) -> ZipListNodeMut {
        self.get_node_mut(self.get_tail_offset())
    }

    pub fn end_mut(&mut self) -> ZipListNodeMut {
        self.get_node_mut(self.0.len())
    }

    pub fn find(&self, v: &[u8]) -> Option<ZipListValue> {
        for n in self.iter_rev() {
            match n {
                ZipListValue::Bytes(b) => {
                    if b == v {
                        return Some(n);
                    }
                }
                ZipListValue::Int(k) => {
                    if k.to_string().as_bytes() == v {
                        return Some(n);
                    }
                }
            }
        }
        None
    }

    pub fn find_mut(&mut self, v: &[u8]) -> Option<ZipListNodeMut> {
        let mut n = self.front_mut();
        while !n.at_end() {
            if n.value() == v {
                return Some(n);
            }
            n = n.move_next();
        }
        None
    }

    pub fn get_value(&self, off: usize) -> Option<ZipListValue> {
        if off == self.0.len() {
            None
        } else {
            Some(Node::new(&self.0[off..]).value())
        }
    }

    pub fn get_node_mut(&mut self, off: usize) -> ZipListNodeMut {
        ZipListNodeMut {
            list: self,
            off,
        }
    }

    fn set_usize_value(&mut self, value: usize, off: usize, n: usize) {
        assert!(n <= mem::size_of::<usize>());
        for i in 0..n {
            let mut v = value >> ((n - i - 1) * 8);
            v &= 0xff;
            self.0[off + i] = v as u8;
        }
    }

    fn set_tail_offset(&mut self, off: usize) {
        self.set_usize_value(off, 0, ZIP_LIST_TAIL_OFF_SIZE);
    }

    fn set_len(&mut self, mut len: usize) {
        if len > std::u16::MAX as usize {
            len = std::u16::MAX as usize;
        }
        self.set_usize_value(len, ZIP_LIST_TAIL_OFF_SIZE, ZIP_LIST_LEN_SIZE);
    }

    fn incr_len(&mut self, by: usize) {
        self.set_len(self.len() + by);
    }

    fn get_usize_value(&self, off: usize, n: usize) -> usize {
        let mut v = 0usize;
        for i in 0..n {
            v <<= 8;
            v |= self.0[off + i] as usize;
        }
        v
    }

    fn get_tail_offset(&self) -> usize {
        self.get_usize_value(0, ZIP_LIST_TAIL_OFF_SIZE)
    }

    fn inner_insert(&mut self, off: usize, s: &[u8]) {
        let next_diff: i64;
        let prev_len;
        let prev_len_size;
        let req_len;
        let old_len;
        let encoding;

        prev_len = if off != self.0.len() {
            decode_prev_length(&self.0[off..])
        } else if self.get_tail_offset() != self.0.len() {
            self.tail_blob_len()
        } else {
            0 // at front
        };

        prev_len_size = prev_length_size(prev_len);

        encoding = match bytes_to_i64(s) {
            Ok(i) => Encoding::Int(i),
            Err(_) => Encoding::Str(s.len()),
        };

        req_len = encoding.blob_len_with_content() + prev_len_size;

        // next diff could be negative
        next_diff = if off != self.0.len() {
            prev_length_size(req_len) as i64 - prev_len_size as i64
        } else {
            0
        };

        old_len = self.0.len();
        // TODO: can write the data here
        self.0.splice(
            off..off,
            (0..(req_len as i64 + next_diff) as usize).map(|_| { 0u8 }),
        );

        if off != old_len {
            write_prev_length(
                req_len,
                &mut self.0[off + req_len..off + req_len + prev_length_size(req_len)],
            );

            self.set_tail_offset(self.get_tail_offset() + req_len);

            if off + req_len != self.get_tail_offset() {
                self.set_tail_offset((self.get_tail_offset() as i64 + next_diff) as usize);
            }
        } else {
            self.set_tail_offset(off);
        }

        if next_diff != 0 {
            self.cascade_update(off + req_len);
        }

        write_prev_length(prev_len, &mut self.0[off..off + prev_len_size]);
        let off = off + prev_len_size;
        encoding.write_with_content(
            &mut self.0[off..off + encoding.blob_len_with_content()],
            s,
        );

        self.incr_len(1);
    }

    fn cascade_update(&mut self, mut off: usize) {
        while off != self.0.len() {
            let curr = Node::new(&self.0[off..]);
            let raw_len: usize = curr.blob_len();
            let raw_len_size: usize = prev_length_size(raw_len);

            if off + raw_len == self.0.len() {
                break;
            }

            let next = Node::new(&self.0[off + raw_len..]);
            let next_prev_raw_len = next.prev_raw_len;
            let next_prev_raw_len_size = next.prev_raw_len_size;

            if next_prev_raw_len == raw_len {
                break;
            }

            if next_prev_raw_len_size < raw_len_size {
                let extra: usize = raw_len_size - next_prev_raw_len_size;
                self.0.splice(
                    off + raw_len..off + raw_len,
                    (0..extra).map(|_| 0u8),
                );

                let next_off: usize = off + raw_len;
                if next_off != self.get_tail_offset() {
                    self.set_tail_offset(self.get_tail_offset() + extra);
                }

                write_prev_length(
                    raw_len,
                    &mut self.0[next_off..next_off + next_prev_raw_len_size + extra],
                );

                off += raw_len;
            } else {
                if next_prev_raw_len_size > raw_len_size {
                    force_write_large_prev_length(
                        raw_len,
                        &mut self.0[off + raw_len..off + raw_len + next_prev_raw_len_size],
                    );
                } else {
                    write_prev_length(
                        raw_len,
                        &mut self.0[off + raw_len..off + raw_len + next_prev_raw_len_size],
                    );
                }
                break;
            }
        }
    }

    fn inner_delete(&mut self, off: usize, num: usize) {
        let mut deleted: usize = 0;
        let mut next_off: usize = off;
        let mut next_diff: i64 = 0;

        for _ in 0..num {
            if next_off == self.0.len() { break; }
            next_off += Node::parse_blob_len(&self.0[next_off..]);
            deleted += 1;
        }

        if next_off > off {
            if next_off != self.0.len() {
                let first_prev_len: usize = decode_prev_length(&self.0[off..]);
                let next_prev_len: usize = decode_prev_length(&self.0[next_off..]);
                next_diff = prev_length_size(first_prev_len) as i64 -
                    prev_length_size(next_prev_len) as i64;

                next_off = (next_off as i64 - next_diff as i64) as usize;

                write_prev_length(
                    first_prev_len,
                    &mut self.0[next_off..next_off + prev_length_size(first_prev_len)],
                );

                self.set_tail_offset(
                    self.get_tail_offset() - (next_off - off)
                );
            } else {
                self.set_tail_offset(
                    off - decode_prev_length(&self.0[off..])
                );
            }
            self.0.drain(off..next_off);
            self.set_len(self.len() - deleted);

            if next_diff != 0 {
                self.cascade_update(off);
            }
        }
    }

    fn tail_blob_len(&self) -> usize {
        Node::parse_blob_len(&self.0[self.get_tail_offset()..])
    }

    fn header_len(&self) -> usize {
        ZIP_LIST_HEADER_SIZE
    }
}