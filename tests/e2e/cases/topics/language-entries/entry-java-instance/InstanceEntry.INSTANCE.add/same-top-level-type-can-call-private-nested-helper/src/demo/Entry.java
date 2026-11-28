package demo;

public class Entry {
    public static int run(int x) { return Tools.value(x); }
    static class Tools { private static int value(int x) { return x + 1; } }
}
