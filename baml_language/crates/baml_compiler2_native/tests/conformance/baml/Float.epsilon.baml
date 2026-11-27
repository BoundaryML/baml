function float_epsilon_value() -> float { float.epsilon() }
function float_epsilon_above_one() -> bool { halves(2) + float.epsilon() > halves(2) }
