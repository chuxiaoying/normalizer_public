class Entry {
    static String read(Object value, boolean stop) {
        if (!(value instanceof String text)) {
            if (stop) throw new IllegalArgumentException("first");
            else throw new IllegalArgumentException("second");
        }
        return text;
    }
}
