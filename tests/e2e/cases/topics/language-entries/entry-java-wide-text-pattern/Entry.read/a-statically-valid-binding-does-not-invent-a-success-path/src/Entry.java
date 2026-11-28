class Entry {
    static String read() {
        Object value = null;
        if (!(value instanceof String text)) throw new IllegalArgumentException("guard");
        return text;
    }
}
