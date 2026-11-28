package demo;

class Tools {
    static int change(int x) { return x + 100; }
}

class Local {
    int change(int x) { return x + 1; }
}

class Entry {
    static int run(int x) {
        Local Tools = new Local();
        return Tools.change(x);
    }
}
