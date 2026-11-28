class Entry {
    static String read(Object value) {
        Object alias = value;
        if (alias instanceof String text) return text;
        return "fallback";
    }
}
