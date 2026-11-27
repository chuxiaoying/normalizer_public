import normalizer.intrinsics.N;

class Profile {
    int score;
}

class ProfileService {
    @N.External
    private final ProfileClient client;

    ProfileService(ProfileClient client) {
        this.client = client;
    }

    int score(int accountId) {
        return client.read(accountId).score;
    }
}

class ProfileClient {
    native Profile read(int accountId) throws ProfileUnavailable;
}

class ProfileUnavailable extends RuntimeException {}

class Beans {
    static final ProfileService ENTRY = new ProfileService(new ProfileClient());
}
