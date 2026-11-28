import normalizer.intrinsics.N;

class Entry {
    @N.EntryState int seen;
    String read(Object value) {
        if (take(value) instanceof String text) return text;
        return "fallback";
    }
    Object take(Object value) {
        seen = seen + 1;
        return value;
    }
}
class Beans {
    static final Entry ENTRY = new Entry();
}
