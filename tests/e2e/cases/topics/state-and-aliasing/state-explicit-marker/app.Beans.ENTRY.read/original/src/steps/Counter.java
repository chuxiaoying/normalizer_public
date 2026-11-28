package steps;

import normalizer.intrinsics.N;

public class Counter {
    @N.EntryState public int units;

    public int read() {
        return this.units;
    }
}
