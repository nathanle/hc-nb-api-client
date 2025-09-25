use serde_json;
use futures::future::join_all;
use std::sync::Arc;
use tokio::sync::Semaphore;
use tokio::task;
use serde::{Serialize};
use clap::Parser;
use rust_decimal::prelude::*;
use chrono::{NaiveDateTime, DateTime, Utc, Local, TimeZone};
use reqwest::header::{HeaderMap, HeaderValue, AUTHORIZATION};
use reqwest::Client;
use std::env;
use std::sync::LazyLock;
use crate::database::{
    create_maindb_client,
    create_localdb_client,
    localdb_init,
    get_nb_ids,
    get_nbcfg_ids,
    get_nb_by_loc,
    update_db_node,
    update_db_nb,
    update_db_config,
    LocalNodeBalancerListObject,
    NodeBalancerListObject,
    NodeBalancerConfigObject,
    NodeObject
};

mod database;


static api_version: LazyLock<String> = LazyLock::new(|| {
    env::var("APIVERSION").expect("APIVERSION not set!") 
});
static token: LazyLock<String> = LazyLock::new(|| {
    env::var("TOKEN").expect("TOKEN not set!")
});

#[derive(Parser, Debug)]
#[command(version, about, long_about = None)]
struct Args {
    #[arg(short, long)]
    data: bool,
}

#[derive(serde::Deserialize, Serialize, Debug)]
struct NodeBalancerListData {
    data: Vec<NodeBalancerListObject>,
    page: u64,
    pages: u64,
    results: u64,
}

#[derive(serde::Deserialize, Serialize, Debug)]
struct NodeBalancerConfigData {
    data: Vec<NodeBalancerConfigObject>,
    page: u64,
    pages: u64,
    results: u64,
}

#[derive(serde::Deserialize, Serialize, Debug)]
struct NodeListData {
    data: Vec<NodeObject>,
    page: u64,
    pages: u64,
    results: u64,
}

fn epoch_to_dt(e: &String) -> String {
    let timestamp = e.parse::<i64>().unwrap();
    let naive = NaiveDateTime::from_timestamp(timestamp, 0);
    let datetime: DateTime<Utc> = DateTime::from_utc(naive, Utc);
    let newdate = datetime.format("%Y-%m-%d %H:%M:%S");

    newdate.to_string()
}

#[tokio::main]
async fn main() -> Result<(), Box<dyn std::error::Error>> {
    let _ = localdb_init().await;
    loop {
        let args = Args::parse();
        let auth_header = format!("Bearer {}", token.to_string());
        let mut headers = HeaderMap::new();
        headers.insert(AUTHORIZATION, HeaderValue::from_str(&auth_header).unwrap());
        headers.insert("accept", HeaderValue::from_static("application/json"));

        let client = Client::builder()
            .default_headers(headers)
            .build()?;

        let loc = env::var("LOCATION").expect("LOCATION not set!");
        let mut connection = create_maindb_client().await;
        let nb_ids = get_nb_by_loc(loc.to_string()).await;
        match nb_ids {
            Ok(ref s) => (),
            Err(ref e) => println!("{:?}", e),
        }
        let unwraped_nb_ids = &nb_ids.unwrap();
        let sem = Arc::new(Semaphore::new(100));
        println!("Processing NBs");
        for x in unwraped_nb_ids.clone() {
            let nbid: i32 = x.get(0);
            let ipv4: String = x.get(1);
            let region: String = x.get(2);
            let lke_id: i32 = x.get(3);
            let nb_payload = LocalNodeBalancerListObject {
                nb_id: nbid,
                ipv4: ipv4,
                region: region,
                lke_id: lke_id,
            };
            let permit = Arc::clone(&sem).acquire_owned().await;
            task::spawn(async move {
                let _permit = permit;
                let _ = update_db_nb(nb_payload).await;
            });
            //let _ = tokio::spawn(
            //    async move {
            //        let _ = update_db_nb(nb_payload).await;
            //    }
            //);
        }

        println!("Processing configs");
        for n in unwraped_nb_ids {
            let nbid: i32 = n.get(0);
            let config_url = format!("https://api.linode.com/{}/nodebalancers/{}/configs", api_version.to_string(), nbid);
            let config_response = client.get(config_url)
                .send()
                .await?;

            //println!("{:#?}", config_response.status());


            if config_response.status().is_success() {
                let json: serde_json::Value = config_response.json().await?;
                let nbconfigdata: NodeBalancerConfigData = serde_json::from_value(json.clone()).unwrap();

                if nbconfigdata.pages == 1 {
                    for d in nbconfigdata.data {
                        let configobj: database::NodeBalancerConfigObject = d;
                        let borrow_configobj = &configobj;
                        let cfgid = borrow_configobj.id;
                        let nbid = borrow_configobj.nodebalancer_id;
                        let _ = tokio::spawn(
                            async move {
                                let _ = update_db_config(configobj).await;
                            }
                        );
                    }
                } else {
                    let mut page = 1;
                    let cfgsem = Arc::new(Semaphore::new(100));
                    while page <= nbconfigdata.pages {
                        println!("Processing page {}", page);
                        let config_url = format!("https://api.linode.com/{}/nodebalancers/{}/configs?page={}", api_version.to_string(), nbid, &page);

                        let config_response = client.get(config_url)
                            .send()
                            .await?;

                        if config_response.status().is_success() {
                            let json: serde_json::Value = config_response.json().await?;
                            let nbconfigdata: NodeBalancerConfigData = serde_json::from_value(json.clone()).unwrap();
                            for d in nbconfigdata.data {
                                let configobj: database::NodeBalancerConfigObject = d;
                                let borrow_configobj = &configobj;
                                let cfgid = borrow_configobj.id;
                                let nbid = borrow_configobj.nodebalancer_id;
                                let permit = Arc::clone(&cfgsem).acquire_owned().await;
                                task::spawn(async move {
                                    let _permit = permit;
                                    let _ = update_db_config(configobj).await;
                                });
                                //let _ = tokio::spawn(
                                //    async move {
                                //        let _ = update_db_config(configobj).await;
                                //    }
                                //);
                            }
                        }
                        page += 1;
                    }
                }
            }
        }
        let nbcfg_ids = get_nbcfg_ids().await;
        println!("Processing nodes");
        for n in nbcfg_ids.unwrap() {
            let cfgid: i32 = n.get(0);
            let nbid: i32 = n.get(1);
            let node_url = format!("https://api.linode.com/{}/nodebalancers/{}/configs/{}/nodes", api_version.to_string(), nbid, cfgid);
            let node_response = client.get(node_url)
                .send()
                .await?;

            if node_response.status().is_success() {
                let json: serde_json::Value = node_response.json().await?;
                let nodedata: NodeListData = serde_json::from_value(json.clone()).unwrap();
                if nodedata.pages == 1 {
                    for d in nodedata.data {
                        let nodeobj: database::NodeObject = d;
                        let _ = tokio::spawn(
                            async move {
                                let _ = update_db_node(nodeobj).await;
                            }
                        );
                    }
                } else {
                    let mut page = 1;
                    let nodesem = Arc::new(Semaphore::new(200));
                    while page <= nodedata.pages {
                        println!("Processing node page {}", page);
                        let node_url = format!("https://api.linode.com/{}/nodebalancers/{}/configs/{}/nodes", api_version.to_string(), nbid, cfgid);
                        let node_response = client.get(node_url)
                            .send()
                            .await?;

                        if node_response.status().is_success() {
                            let json: serde_json::Value = node_response.json().await?;
                            let nodedata: NodeListData = serde_json::from_value(json.clone()).unwrap();
                            for d in nodedata.data {
                                let nodeobj: database::NodeObject = d;
                                let permit = Arc::clone(&nodesem).acquire_owned().await;
                                task::spawn(async move {
                                    let _permit = permit;
                                    let _ = update_db_node(nodeobj).await;
                                });
                                //let _ = tokio::spawn(
                                //    async move {
                                //        let _ = update_db_node(nodeobj).await;
                                //    }
                                //);
                            }
                        }
                        page += 1;
                    }
                }
            }
        }

        if args.data {
            let mut connection = create_localdb_client().await;
            let rows = connection.query("SELECT * FROM node JOIN nodebalancer ON node.nodebalancer_id = nodebalancer.nb_id JOIN nodebalancer_config ON nodebalancer_config.nodebalancer_id = nodebalancer.nb_id;", &[]).await?;
                // Print header
            println!("{:<10} {:<23} {:<6} {:<10} {:<6} {:<15} {:<15} {:<10} {:<5} {:<3} {:<3}", "ID", "Address", "Status", "Config ID", "NB ID", "IPv4 VIP", "Region", "Algorithm", "Port", "Up", "Down");
            println!("--------------------------------------------------------------------------------------------------------------------");
            //println!("{:#?}", rows);

            // Iterate over the rows and print data
            for row in rows {
                let id: i32 = row.get(0);
                let address: String = row.get(1);
                let status: String = row.get(2);
                let config_id: i32 = row.get(3);
                let nb_id: i32 = row.get(4);
                let nb_id_none: i32 = row.get(5);
                let vip: String = row.get(6);
                let nbregion: String = row.get(7);
                let lke_id: i32 = row.get(8);
                let c_id: i32 = row.get(9);
                let algorithm: String = row.get(10);
                let port: i32 = row.get(11);
                let up: i32 = row.get(12);
                let down: i32 = row.get(13);
                let n_id: i32 = row.get(14);
                println!("{:<10} {:<23} {:<6} {:<10} {:<6} {:<15} {:<15} {:<10} {:<5} {:<3} {:<3}", id, address, status, config_id, nb_id, vip, nbregion, algorithm, port, up, down);
            }
        }

    }
}
