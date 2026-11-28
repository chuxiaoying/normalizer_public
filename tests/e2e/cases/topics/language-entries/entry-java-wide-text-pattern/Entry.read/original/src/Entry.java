class Entry {
    static String read(Object value) {
        if (value instanceof String text) return text;
        throw new IllegalArgumentException("Expected text");
    }
}
