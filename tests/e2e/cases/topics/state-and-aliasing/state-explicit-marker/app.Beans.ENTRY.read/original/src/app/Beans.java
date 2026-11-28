package app;

import steps.Counter;

public class Beans {
    static final Counter ENTRY = new Counter();

    static {
        ENTRY.units = 10;
    }
}
