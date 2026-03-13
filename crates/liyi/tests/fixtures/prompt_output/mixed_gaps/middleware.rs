// @liyi:requirement(auth-check)
// All API endpoints must verify user authentication.
// @liyi:end-requirement(auth-check)

// @liyi:requirement(rate-limit)
// Requests must be rate limited per user.
// @liyi:end-requirement(rate-limit)

// @liyi:related auth-check
fn verify_session() {
    // check session token
}




fn unrelated_helper() {
    // does not reference auth-check
}
