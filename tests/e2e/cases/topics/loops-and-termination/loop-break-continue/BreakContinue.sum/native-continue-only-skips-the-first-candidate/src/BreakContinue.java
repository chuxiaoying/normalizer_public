class BreakContinue {
    static int sum(int limit) {
        int total = 0;
        for (int i = 0; i < limit; i++) {
            if (i < 1) continue;
            if (i > 4) break;
            total = total + i;
        }
        return total;
    }
}
