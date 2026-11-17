# Empacota o binário estático da release (scripts/package.sh rpm|srpm troca @VERSION@).
%global debug_package %{nil}

Name:           pmog
Version:        @VERSION@
Release:        1%{?dist}
Summary:        Jogo de terminal multiplayer na rede local
License:        LicenseRef-Proprietary
URL:            https://github.com/Parsec-Inc/pmog
Source0:        pmog-linux
Source1:        pmog.xml
Source2:        ufw-pmog
Source3:        pmog-linux.sig
ExclusiveArch:  x86_64

%description
Jogo de terminal multiplayer na rede local. Traz o serviço pmog do firewalld
e o perfil do ufw; liberar as portas fica a cargo do usuário.

%install
install -Dm755 %{SOURCE0} %{buildroot}%{_bindir}/pmog
install -Dm644 %{SOURCE3} %{buildroot}%{_bindir}/pmog.sig
install -Dm644 %{SOURCE1} %{buildroot}%{_prefix}/lib/firewalld/services/pmog.xml
install -Dm644 %{SOURCE2} %{buildroot}%{_sysconfdir}/ufw/applications.d/pmog

%files
%{_bindir}/pmog
%{_bindir}/pmog.sig
%{_prefix}/lib/firewalld/services/pmog.xml
%config(noreplace) %{_sysconfdir}/ufw/applications.d/pmog
