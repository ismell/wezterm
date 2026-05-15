use super::*;
use mlua::UserDataRef;
use mux::domain::{Domain, DomainId, DomainState};
use std::sync::Arc;

#[derive(Clone, Copy, Debug)]
pub struct MuxDomain(pub DomainId);

impl MuxDomain {
    pub fn resolve<'a>(&self, mux: &'a Arc<Mux>) -> mlua::Result<Arc<dyn Domain>> {
        mux.get_domain(self.0)
            .ok_or_else(|| mlua::Error::external(format!("domain id {} not found in mux", self.0)))
    }
}

impl UserData for MuxDomain {
    fn add_methods<'lua, M: UserDataMethods<'lua, Self>>(methods: &mut M) {
        methods.add_meta_method(mlua::MetaMethod::ToString, |_, this, _: ()| {
            Ok(format!("MuxDomain(pane_id:{}, pid:{})", this.0, unsafe {
                libc::getpid()
            }))
        });
        methods.add_method("domain_id", |_, this, _: ()| Ok(this.0));

        methods.add_method("is_spawnable", |_, this, _: ()| {
            let mux = get_mux()?;
            let domain = this.resolve(&mux)?;
            Ok(domain.spawnable())
        });

        methods.add_async_method(
            "attach",
            |_, this, window: Option<UserDataRef<MuxWindow>>| async move {
                let mux = get_mux()?;
                let domain = this.resolve(&mux)?;
                domain.attach(window.map(|w| w.0)).await.map_err(|err| {
                    mlua::Error::external(format!(
                        "failed to attach domain {}: {err:#}",
                        domain.domain_name()
                    ))
                })
            },
        );

        methods.add_method("detach", |_, this, _: ()| {
            let mux = get_mux()?;
            let domain = this.resolve(&mux)?;
            domain.detach().map_err(|err| {
                mlua::Error::external(format!(
                    "failed to detach domain {}: {err:#}",
                    domain.domain_name()
                ))
            })
        });

        methods.add_method("state", |_, this, _: ()| {
            let mux = get_mux()?;
            let domain = this.resolve(&mux)?;
            Ok(match domain.state() {
                DomainState::Attached => "Attached",
                DomainState::Detached => "Detached",
            })
        });

        methods.add_method("name", |_, this, _: ()| {
            let mux = get_mux()?;
            let domain = this.resolve(&mux)?;
            Ok(domain.domain_name().to_string())
        });

        methods.add_async_method("label", |_, this, _: ()| async move {
            let mux = get_mux()?;
            let domain = this.resolve(&mux)?;
            Ok(domain.domain_label().await)
        });

        methods.add_async_method(
            "add_port_forward",
            |_, this, args: mlua::Table| async move {
                let mux = get_mux()?;
                let domain = this.resolve(&mux)?;

                let typ: String = args.get("type")?;

                let pf = match typ.as_str() {
                    "local" => {
                        let local_host: String = args.get("local_host")?;
                        let local_port: u16 = args.get("local_port")?;
                        let remote_host: String = args.get("remote_host")?;
                        let remote_port: u16 = args.get("remote_port")?;
                        wezterm_ssh::PortForward::Local {
                            local_host,
                            local_port,
                            remote_host,
                            remote_port,
                        }
                    }
                    "remote" => {
                        let remote_host: Option<String> = args.get("remote_host")?;
                        let remote_port: u16 = args.get("remote_port")?;
                        let local_host: String = args.get("local_host")?;
                        let local_port: u16 = args.get("local_port")?;
                        wezterm_ssh::PortForward::Remote {
                            remote_host,
                            remote_port,
                            local_host,
                            local_port,
                        }
                    }
                    _ => return Err(mlua::Error::external("Invalid forward type")),
                };

                if let Some(ssh_dom) = domain.downcast_ref::<mux::ssh::RemoteSshDomain>() {
                    let addr = ssh_dom
                        .add_port_forward(pf)
                        .await
                        .map_err(|e| mlua::Error::external(format!("{:#}", e)))?;
                    Ok(addr.to_string())
                } else {
                    Err(mlua::Error::external("Domain is not an SSH domain"))
                }
            },
        );

        methods.add_method("has_any_panes", |_, this, _: ()| {
            let mux = get_mux()?;
            let domain = this.resolve(&mux)?;
            let have_panes_in_domain = mux
                .iter_panes()
                .iter()
                .any(|p| p.domain_id() == domain.domain_id());
            Ok(have_panes_in_domain)
        });
    }
}
