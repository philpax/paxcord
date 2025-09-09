# paxcord

<img src="docs/paxcord.png" alt="paxcord logo: a purple crystal" width="256" />

My personal Discord bot.

## Setup

### Bot

#### Discord

- [Create a Discord application](https://discord.com/developers/applications) and fill it out with your own details.
- Go to `Bot` and create a new Bot.
  - Hit `Reset Token`, and copy the token it gives you somewhere.
- Go to `OAuth2 > URL Generator`, select `bot`, then select `Send Messages` and `Use Slash Commands`.
  - Go to the URL it generates, and then invite it to a server of your choice.

#### Application

- Install Rust 1.87 or above using `rustup`.
- Run `cargo run --release` to start paxcord. This will auto-generate a configuration file, and then quit.
- Fill in the configuration file with the required details.
- You can then run paxcord to your heart's content.
