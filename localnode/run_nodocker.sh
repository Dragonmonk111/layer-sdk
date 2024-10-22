rm -rf ${HOME}/.slay3r
mkdir -p ${HOME}/.slay3r

cometbft init --home ~/.slay3r

cp -a ./comet/ ${HOME}/.slay3r
cp ./abci/config/slay3r.toml ${HOME}/.slay3r/config

# replace root with user directory
gsed -i '/rocksdb/d' ${HOME}/.slay3r/config/slay3r.toml
sed -i '/rocksdb/d' ${HOME}/.slay3r/config/slay3r.toml

echo -n 'rocksdb = "' >> ${HOME}/.slay3r/config/slay3r.toml
echo -n "$HOME" >> ${HOME}/.slay3r/config/slay3r.toml
echo -n '/.slay3r/data/app"' >> ${HOME}/.slay3r/config/slay3r.toml

slay3rd --log debug

# run this on a different terminal
# cometbft start --home $HOME/.slay3r
