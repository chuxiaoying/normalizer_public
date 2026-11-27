class PrefixSelection {
    static int firstAbove(int cutoff, int fallback) {
        int[] tariff = {7, 2, 0, 99};
        for (int i = 0; i < 4; i++) {
            int value = i == 0 ? tariff[0] : i == 1 ? tariff[1] : i == 2 ? tariff[2] : tariff[3];
            if (value == 0) return fallback;
            if (value > cutoff) return value;
        }
        return fallback;
    }
}
