package steps;

import normalizer.intrinsics.N;

public class Counter {
    @N.EntryState private int units;

    public int read() {
        return this.units;
    }
}
