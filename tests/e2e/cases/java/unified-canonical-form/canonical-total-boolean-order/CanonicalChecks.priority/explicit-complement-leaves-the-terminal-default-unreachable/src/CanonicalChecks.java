class CanonicalChecks {
    static int priority(boolean a, boolean b, boolean c) {
        if (a) return 10;
        if (b) return 20;
        if (c) return 10;
        if (!c) return 30; return 40;
    }
}
