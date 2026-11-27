class Pricing {
    static int normalize(int value) {
        return value + 1;
    }

    static int addBase(int value, int base) {
        return value + base;
    }

    static int cap(int value, int limit) {
        return value > limit ? limit : value;
    }

    static int calculate(int value, int base, int limit) {
        int normalized = normalize(value);
        int capped = cap(normalized, limit);
        return addBase(capped, base);
    }
}
