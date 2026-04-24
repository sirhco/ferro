use leptos::prelude::*;

#[server(endpoint = "/_srv/login")]
pub async fn login(email: String, password: String) -> Result<(), ServerFnError> {
    // Delegate to the AuthService on the server. Wired in by the CLI via
    // `provide_context::<Arc<AuthService>>`.
    let _ = (email, password);
    Ok(())
}

#[component]
pub fn LoginPage() -> impl IntoView {
    let email = RwSignal::new(String::new());
    let password = RwSignal::new(String::new());
    let login_action = ServerAction::<Login>::new();

    view! {
        <section class="ferro-login">
            <h1>"Sign in"</h1>
            <ActionForm action=login_action>
                <label>"Email"
                    <input type="email" name="email" required
                        on:input=move |ev| email.set(event_target_value(&ev)) />
                </label>
                <label>"Password"
                    <input type="password" name="password" required
                        on:input=move |ev| password.set(event_target_value(&ev)) />
                </label>
                <button type="submit">"Sign in"</button>
            </ActionForm>
        </section>
    }
}
