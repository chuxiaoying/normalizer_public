class PostTest {
    static int run(boolean active) {
        int total = 0;
        do {
            switch (1) {
                case 1:
                    total = 2;
                    if (active) continue;
                    break;
                default: total = 3;
            }
            total += 7;
        } while (false);
        return total;
    }
}
