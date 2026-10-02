using CsharpBasicCalls;

// Public SDK conformance; names mirror the other languages' invocation_options.
internal static class invocation_options
{
    internal static void four_call_forms()
    {
        var opts = new BamlOptions { TimeoutMs = 1000 };
        Equal(Functions.OptionalArgsProbe(1), 1, 5, 99);
        Equal(Functions.OptionalArgsProbe(1, opt1: 7L), 1, 7, 99);
        Equal(Functions.OptionalArgsProbe(1, baml: opts), 1, 5, 99);
        Equal(Functions.OptionalArgsProbe(1, opt1: 7L, baml: opts), 1, 7, 99);
        Console.WriteLine("invocation_options.four_call_forms=ok");
    }

    internal static async Task four_call_forms_async()
    {
        var opts = new BamlOptions { TimeoutMs = 1000 };
        Equal(await Functions.OptionalArgsProbeAsync(1), 1, 5, 99);
        Equal(await Functions.OptionalArgsProbeAsync(1, opt1: 7L), 1, 7, 99);
        Equal(await Functions.OptionalArgsProbeAsync(1, baml: opts), 1, 5, 99);
        Equal(await Functions.OptionalArgsProbeAsync(1, opt1: 7L, baml: opts), 1, 7, 99);
        Console.WriteLine("invocation_options.four_call_forms_async=ok");
    }

    internal static void omitted_argument_is_not_null()
    {
        Equal(Functions.OptionalArgsProbe(1, opt1: (long?)null, baml: new BamlOptions()), 1, null, 99);
        Console.WriteLine("invocation_options.omitted_argument_is_not_null=ok");
    }

    internal static void explicit_controls_are_applied()
    {
        foreach (bool supplyOptional in new[] { false, true })
        {
            try
            {
                var expired = new BamlOptions { TimeoutMs = 0 };
                if (supplyOptional) Functions.OptionalArgsProbe(1, opt1: 7L, baml: expired);
                else Functions.OptionalArgsProbe(1, baml: expired);
            }
            catch (OperationCanceledException)
            {
                continue;
            }
            throw new InvalidOperationException("Explicit invocation controls were ignored");
        }
        Console.WriteLine("invocation_options.explicit_controls_are_applied=ok");
    }

    private static void Equal(IReadOnlyList<long?> actual, params long?[] expected)
    {
        if (!actual.SequenceEqual(expected))
            throw new InvalidOperationException($"Invocation result: [{string.Join(",", actual)}]");
    }

    internal static async Task explicit_controls_are_applied_async()
    {
        foreach (bool supplyOptional in new[] { false, true })
        {
            try
            {
                var expired = new BamlOptions { TimeoutMs = 0 };
                if (supplyOptional) await Functions.OptionalArgsProbeAsync(1, opt1: 7L, baml: expired);
                else await Functions.OptionalArgsProbeAsync(1, baml: expired);
            }
            catch (OperationCanceledException)
            {
                continue;
            }
            throw new InvalidOperationException("Explicit async invocation controls were ignored");
        }
        Console.WriteLine("invocation_options.explicit_controls_are_applied_async=ok");
    }
}
