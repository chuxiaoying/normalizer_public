class HelloWorld {
    static int addOne(int value) {
        int next = value + 1;
        if (next <= value) {
            return 99;
        }
        return next;
    }
}
