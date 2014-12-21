use time::Timespec;

pub fn add(a: Timespec, b: Timespec) -> Timespec {
    let nsec = a.nsec as i64 + b.nsec as i64;
    let sectoadd = nsec / 1000000000i64;

    Timespec {
        sec:    sectoadd + a.sec + b.sec,
        nsec:   (nsec - (sectoadd * 1000000000i64)) as i32,
    }
}