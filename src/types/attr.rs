use fuser::FileAttr;
use std::time::SystemTime;

fn base_attr(ino: u64, size: u64) -> FileAttr {
    let now = SystemTime::now();
    FileAttr {
        ino,
        size,
        blocks: size.div_ceil(512),
        atime: now,
        mtime: now,
        ctime: now,
        crtime: now,
        kind: fuser::FileType::RegularFile,
        perm: 0o444,
        nlink: 1,
        uid: 1000,
        gid: 1000,
        rdev: 0,
        flags: 0,
        blksize: 512,
    }
}

pub fn default_file_attr(ino: u64, size: u64) -> FileAttr {
    base_attr(ino, size)
}

pub fn default_dir_attr(ino: u64) -> FileAttr {
    let mut attr = base_attr(ino, 0);
    attr.kind = fuser::FileType::Directory;
    attr.perm = 0o755;
    attr.nlink = 2;
    attr.blocks = 0;
    attr
}
