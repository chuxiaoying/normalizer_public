import normalizer.intrinsics.N;

class Account {
    @N.EntryState @N.Nullable String label;
    String read(String fallback) {
        String previous = label;
        label = null;
        return previous == null ? fallback : previous;
    }
}
class Beans {
    static final Account ENTRY = new Account();
}
