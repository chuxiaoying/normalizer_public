import normalizer.intrinsics.N;

class Profile {
    int score;

    int getScore() { return score; }
    void setScore(int input) { this.score = input; }
}

class ProfileService {
    @N.External
    private final ProfileClient client;

    ProfileService(ProfileClient client) {
        this.client = client;
    }

    int score(int accountId) {
        Profile previous = client.preview(accountId); Profile selected = client.fetch(accountId); return previous.getScore() - selected.getScore();
    }
}

class ProfileClient {
    native Profile preview(int accountId); native Profile fetch(int accountId) throws ProfileUnavailable;
}

class ProfileUnavailable extends RuntimeException {}

class Beans {
    static final ProfileService ENTRY = new ProfileService(new ProfileClient());
}
