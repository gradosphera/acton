struct Profile {
    id: int
    points: int
}

fun onInternalMessage(_in: InMessage) {
    val profile = Profile { id: 7, points: 42 };
    debug.print(profile.points);
}
