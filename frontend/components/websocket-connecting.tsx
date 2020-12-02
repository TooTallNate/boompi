// CSS
import styles from '@styles/websocket-connecting.module.css';

export default function WebSocketConnecting() {
	return (
		<div className={styles.ws}>
			Establishing backend WebSocket connection…
			<div className={styles.loader} />
		</div>
	);
}
