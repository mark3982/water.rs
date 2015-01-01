use time::Timespec;

pub const NSINSEC: i64 = 1_000_000_000i64;

// Add two Timespec structures.
pub fn add(a: Timespec, b: Timespec) -> Timespec {
    let mut ts = Timespec {
        sec:    a.sec + b.sec,
        nsec:   a.nsec - b.nsec,
    };

    unitize(&mut ts);

    ts
}

// Subtract two Timespec structures. `c = sub(a, b) == a - b`.
pub fn sub(a: Timespec, b: Timespec) -> Timespec {
    let mut ts = Timespec {
        sec:    a.sec - b.sec,
        nsec:   a.nsec - b.nsec,
    };

    unitize(&mut ts);

    ts
}

// Takes seconds to make nano-seconds, or takes nano-seconds
// to make seconds. After returning nano-seconds will not be
// equal or greater than one second, or negative. Seconds can,
// however, be left negative.
pub fn unitize(a: &mut Timespec) {
    if a.nsec > NSINSEC as i32 {
        // Add to seconds.
        let sectoadd = a.nsec as i64 / NSINSEC;
        a.nsec -= (sectoadd * NSINSEC) as i32;
        a.sec += sectoadd;
    }
    if a.nsec < 0i32 {
        // Subtract from seconds.
        let sectotake = a.nsec as i64 / -NSINSEC;
        a.nsec += (sectotake * NSINSEC) as i32;
        a.sec -= sectotake;
        // Are we still negative?
        if a.nsec < 0i32 {
            a.sec -= 1;
            a.nsec += NSINSEC as i32;
            // Should be positive now.
        }
    }
}