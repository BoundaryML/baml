function int_ilog_exact() -> int { count(1000).ilog(count(10)) }
function int_ilog_below() -> int { count(999).ilog(count(10)) }
function int_ilog_one() -> int { count(1).ilog(count(2)) }
function int_ilog_base_above() -> int { count(5).ilog(count(9)) }
function int_ilog_large() -> int { (wide(61) - 1 + wide(61)).ilog(count(2)) }
function int_ilog_zero() -> int { count(0).ilog(count(2)) }
function int_ilog_negative() -> int { (0 - count(8)).ilog(count(2)) }
function int_ilog_base_one() -> int { count(8).ilog(count(1)) }
function int_ilog_base_negative() -> int { count(8).ilog((0 - count(2))) }
