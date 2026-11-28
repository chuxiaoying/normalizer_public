package demo;

public class Entry {
    static class Read {
        private static int apply(int value) { return value + 1; }
    }
    static class Call {
        static int use(int value) { return Read.apply(value); }
    }
    public static int run(int value) { return Call.use(value); }
}
