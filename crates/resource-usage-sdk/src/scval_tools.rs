use soroban_client::xdr::ScVal;

pub fn scval_as_string(v: &ScVal) -> Option<String> {
    match v {
        ScVal::Symbol(s) => Some(s.0.to_string()),
        ScVal::String(s) => Some(s.0.to_string()),
        _ => None,
    }
}

pub fn scval_as_u64(v: &ScVal) -> Option<u64> {
    match v {
        ScVal::U32(n) => Some(*n as u64),
        ScVal::I32(n) => (*n).try_into().ok(),
        ScVal::U64(n) => Some(*n),
        ScVal::I64(n) => (*n).try_into().ok(),
        ScVal::I128(parts) => {
            if parts.hi == 0 {
                Some(parts.lo)
            } else {
                None
            }
        }
        ScVal::U128(parts) => {
            if parts.hi == 0 {
                Some(parts.lo)
            } else {
                None
            }
        }
        _ => None,
    }
}
