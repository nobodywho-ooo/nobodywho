using NobodyWho.Demo.Components;
using NobodyWho.Demo.Services;

var builder = WebApplication.CreateBuilder(args);

builder.Services.Configure<DemoOptions>(builder.Configuration.GetSection("NobodyWho"));
builder.Services.AddSingleton<ModelHub>();
builder.Services.AddRazorComponents()
    .AddInteractiveServerComponents();

var app = builder.Build();

if (!app.Environment.IsDevelopment())
{
    app.UseExceptionHandler("/Error", createScopeForErrors: true);
}
app.UseStatusCodePagesWithReExecute("/not-found", createScopeForStatusCodePages: true);

app.UseAntiforgery();

app.MapStaticAssets();
app.MapRazorComponents<App>()
    .AddInteractiveServerRenderMode();

app.Run();
