function float_unit_from_draw_zero() -> float { float._unit_from_draw(count(0)) }
function float_unit_from_draw_small() -> float { float._unit_from_draw(count(1023)) }
function float_unit_from_draw_one_step() -> float { float._unit_from_draw(count(1024)) }
function float_unit_from_draw_large() -> float { float._unit_from_draw(count(1000) * count(1000) * count(1000) * count(1000)) }
function float_unit_from_draw_negative() -> float { float._unit_from_draw(0 - count(1)) }
