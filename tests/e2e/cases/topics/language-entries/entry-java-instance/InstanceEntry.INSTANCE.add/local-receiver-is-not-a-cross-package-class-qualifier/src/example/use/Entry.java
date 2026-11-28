package example.use;

import example.first.Tools;

class Local {
    int change(int units) { return units + 1; }
}

public class Entry {
    public static int run(int units) {
        Local Tools = new Local();
        return Tools.change(units);
    }
}
