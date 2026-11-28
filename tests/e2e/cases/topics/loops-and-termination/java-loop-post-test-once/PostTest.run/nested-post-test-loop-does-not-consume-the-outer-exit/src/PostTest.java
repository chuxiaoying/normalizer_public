class PostTest {
    static int run(boolean active) {
        int total = 0;
        do {
            do {
                total = 2;
                break;
            } while (false);
            if (active) break;
            total += 7;
        } while (false);
        return total;
    }
}
