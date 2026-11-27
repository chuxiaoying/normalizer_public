class Pricing {
    static int normalize(int value) {
        return value + 1;
    }

    static int addBase(int value, int base) {
        return value + base;
    }

    static int cap(int value, int limit) {
        return value < limit ? value : limit;
    }

    static int calculate(int value, int base, int limit) {
        int normalized = normalize(value);
        int withBase = addBase(normalized, base);
        return cap(withBase, limit);
    }
}
