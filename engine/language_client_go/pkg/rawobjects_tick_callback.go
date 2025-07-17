package baml

import (
	"context"
)

type TickCallback func(ctx context.Context, reason TickReason, log FunctionLog) FunctionSignal

type FunctionSignal interface{}
