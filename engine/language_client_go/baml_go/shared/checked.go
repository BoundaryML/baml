package shared

// Check corresponds to the Python Check model.
type Check struct {
	Name       string `json:"name"`
	Expression string `json:"expression"`
	Status     string `json:"status"`
}

// Checked is a generic struct that contains a value of any type T and a map of checks,
// where the key type CN has an underlying type string.
type Checked[T any] struct {
	Value  T                `json:"value"`
	Checks map[string]Check `json:"checks"`
}
