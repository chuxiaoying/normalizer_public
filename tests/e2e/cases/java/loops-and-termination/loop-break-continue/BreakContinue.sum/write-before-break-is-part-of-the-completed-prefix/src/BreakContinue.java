class BreakContinue {
    static int sum(int limit) {
        int total = 0;
        for (int i = 0; i < limit; i++) {
            if (i < 2) continue;
            total = total + i;
            if (i > 4) break;
        }
        return total;
    }
}
