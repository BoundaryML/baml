interface I {
    type X =
        // c
        int;
    type Y extends Iterable
        // c
        = int;
}

class K {
    implements I {
        type X =
            // c
            int;
    }
}
