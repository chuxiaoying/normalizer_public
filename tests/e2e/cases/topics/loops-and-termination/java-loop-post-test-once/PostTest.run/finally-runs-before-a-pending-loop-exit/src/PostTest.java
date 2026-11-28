class PostTest {
    static int run(boolean active) {
        int[] total = {0};
        do {
            try {
                if (active) break;
                total[0] = 1;
            } finally {
                total[0] += 2;
            }
        } while (false);
        return total[0];
    }
}
